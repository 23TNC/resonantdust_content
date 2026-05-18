//! Recipe tape — statement-array recipe form with iterator-aware paths.
//!
//! Replaces the typed `RecipeDef` shape (see [`crate::recipe_core`]) with
//! a uniform statement-tape representation. Two arrays per recipe:
//!
//! - **`input`** — predicates evaluated as a conjunction at match
//!   (client) / verify (server) time.
//! - **`output`** — ordered tape walked sequentially at completion,
//!   with variables (`var.N`), system slots (`sys.duration`,
//!   `sys.style`), conditional gates (`when.<predicate>.<statement>`),
//!   and effect ops (`destroy`, `create`, `set`, `add`, `sub`,
//!   `random`, `aspect`).
//!
//! Path-first grammar: every statement is `<path>.<op>[: value]`.
//! Slot references (`up.slot.N` / `down.slot.N`) are resolved at parse
//! time into iterator IDs, and the parser surfaces a per-recipe list
//! of iterators that the client matcher walks as nested loops. Server
//! receives the matcher's chosen offsets and skips iteration —
//! verification is a linear pass per predicate.
//!
//! JSON shape (per recipe):
//! ```json
//! {
//!   "input":  ["up.slot.0.def_id: corpus", ...],
//!   "output": ["sys.duration.set: 10", "up.slot.0.destroy", ...]
//! }
//! ```
//!
//! See `content/recipes/data/01-test.json` and `02-test.json` for live
//! examples driving these tests.

use serde_json::Value;

use crate::recipe_statement::{parse_statement, Segment, StatementValue};

// ---------- Public types ----------

/// Which side of a card's stack an [`Iterator`] iterates over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(feature = "js", serde(rename_all = "lowercase"))]
pub enum Direction {
  Up,
  Down,
}

/// Resolved path segment. After parsing, every `up.slot.N` /
/// `down.slot.N` triplet in the source path has been collapsed into a
/// single [`Seg::Slot`] referencing an entry in the recipe's
/// [`Recipe::iterators`] list.
///
/// Serialized to JS in adjacent-tagged form: `{ type, value }` where
/// `type` is `"word" | "index" | "slot"`. Lets TS discriminate variants
/// on the `type` field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(
  feature = "js",
  serde(tag = "type", content = "value", rename_all = "camelCase")
)]
pub enum Seg {
  /// Plain identifier — `hex`, `root`, `owner`, `def_id`, `aspect`,
  /// `inventory`, `sys`, `var`, `when`, op tokens (`set`, `add`,
  /// `sub`, `min`, `gt`, …), aspect names, etc.
  Word(String),
  /// Integer path segment — variable index in `var.N`, comparison
  /// value in `when.X.gt.N`, etc. Distinct from values that appear
  /// after the `: ` separator.
  Index(u32),
  /// `slot.N` reference resolved to an iterator + offset. The slot
  /// offset is the literal `N` from the source; the iterator id
  /// indexes into [`Recipe::iterators`].
  Slot {
    #[cfg_attr(feature = "js", serde(rename = "iteratorId"))]
    iterator_id: u32,
    offset: u32,
  },
}

/// One sliding-window iterator over a card's up/down stack.
///
/// Identified by its **parent path** (the resolved segments before
/// the direction marker `up` / `down`) and direction. The parent
/// path is empty for top-level iterators (whose parent is the
/// implicit action anchor); deeper iterators reference earlier ones
/// via [`Seg::Slot`] in their parent path.
///
/// Two source-level slot references with the same resolved parent
/// and direction share the same iterator id; the offset distinguishes
/// which slot within the iterator's window they bind to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(feature = "js", serde(rename_all = "camelCase"))]
pub struct Iterator {
  pub parent: Vec<Seg>,
  pub direction: Direction,
}

/// One parsed statement — either an input predicate or an output
/// tape entry. The parser treats both uniformly; downstream code
/// (matcher / verifier / executor) interprets by structural pattern
/// in the segments.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
pub struct Stmt {
  pub segments: Vec<Seg>,
  pub value: Option<StatementValue>,
}

/// Top-level anchors a recipe references. Used by the client matcher
/// as a pre-filter — recipes whose [`AnchorSet`] isn't a subset of the
/// engagement's available anchors are skipped without iteration.
///
/// Only **top-level** references count:
/// - `hex` / `root` if any statement's path starts with that segment.
/// - `up` / `down` if any iterator's parent is empty and direction
///   matches. Nested iterators (parent referencing an outer slot)
///   don't contribute; they're parameterized on the outer binding
///   and require no engagement-level anchor of their own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
pub struct AnchorSet {
  pub hex: bool,
  pub root: bool,
  pub up: bool,
  pub down: bool,
}

impl AnchorSet {
  /// Bitmask in priority order: `hex << 3 | root << 2 | up << 1 | down`.
  pub fn mask(&self) -> u8 {
    ((self.hex as u8) << 3)
      | ((self.root as u8) << 2)
      | ((self.up as u8) << 1)
      | (self.down as u8)
  }

  /// Number of anchors present.
  pub fn count(&self) -> u8 {
    self.mask().count_ones() as u8
  }

  /// Sort key for priority-tiered matching. Higher key = higher
  /// priority. Primary order: anchor count (longer first). Secondary:
  /// presence of higher-priority anchors (hex > root > up > down).
  ///
  /// Examples: `{hex,root,up,down}` = 79, `{root,up,down}` = 55,
  /// `{hex}` = 24. So `{root,up,down}` (3 anchors, no hex) outranks
  /// `{hex}` (1 anchor) — length first, anchor priority second.
  pub fn priority_key(&self) -> u32 {
    let m = self.mask();
    ((self.count() as u32) << 4) | (m as u32)
  }

  /// True iff every anchor required by `self` is present in `available`.
  pub fn is_subset_of(&self, available: &AnchorSet) -> bool {
    (self.mask() & !available.mask()) == 0
  }
}

/// A parsed recipe.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
pub struct Recipe {
  pub id: String,
  pub input: Vec<Stmt>,
  pub output: Vec<Stmt>,
  /// Iterators in source-order of first appearance. Iterator `n`'s
  /// parent path may reference iterators `0..n-1` via [`Seg::Slot`]
  /// but never iterator `n` or later — guaranteed by construction
  /// (the parent is captured from the already-resolved prefix at the
  /// moment the iterator is created).
  pub iterators: Vec<Iterator>,
  /// Top-level anchors the recipe requires. Computed at parse time;
  /// the client matcher uses this to skip whole recipes before
  /// iterating offsets.
  pub anchors: AnchorSet,
}

// ---------- Parser ----------

/// Parse one recipe's `{ "input": [...], "output": [...] }` JSON
/// object into a [`Recipe`]. Statement strings are lexed via
/// [`crate::recipe_statement::parse_statement`]; the resulting
/// segments are post-processed to resolve `up.slot.N` /
/// `down.slot.N` patterns into iterator IDs.
pub fn parse_recipe(id: &str, value: &Value) -> Result<Recipe, String> {
  let obj = value.as_object().ok_or_else(|| {
    format!(
      "recipe {id:?}: top-level must be an object with `input` and `output` arrays"
    )
  })?;
  let input_raw = obj
    .get("input")
    .ok_or_else(|| format!("recipe {id:?}: missing `input` array"))?;
  let output_raw = obj
    .get("output")
    .ok_or_else(|| format!("recipe {id:?}: missing `output` array"))?;
  let input_arr = input_raw.as_array().ok_or_else(|| {
    format!("recipe {id:?}: `input` must be an array of statement strings")
  })?;
  let output_arr = output_raw.as_array().ok_or_else(|| {
    format!("recipe {id:?}: `output` must be an array of statement strings")
  })?;

  let mut iterators: Vec<Iterator> = Vec::new();
  let mut input = Vec::with_capacity(input_arr.len());
  for (i, item) in input_arr.iter().enumerate() {
    let s = item.as_str().ok_or_else(|| {
      format!("recipe {id:?}: input[{i}] is not a string")
    })?;
    let stmt = parse_one(s, &mut iterators)
      .map_err(|e| format!("recipe {id:?}: input[{i}] {s:?}: {e}"))?;
    input.push(stmt);
  }
  let mut output = Vec::with_capacity(output_arr.len());
  for (i, item) in output_arr.iter().enumerate() {
    let s = item.as_str().ok_or_else(|| {
      format!("recipe {id:?}: output[{i}] is not a string")
    })?;
    let stmt = parse_one(s, &mut iterators)
      .map_err(|e| format!("recipe {id:?}: output[{i}] {s:?}: {e}"))?;
    output.push(stmt);
  }
  let anchors = classify_anchors(&input, &output, &iterators);
  Ok(Recipe {
    id: id.to_string(),
    input,
    output,
    iterators,
    anchors,
  })
}

/// Walk a parsed recipe's statements + iterators and surface the
/// top-level anchors it references. See [`AnchorSet`] for what counts.
fn classify_anchors(
  input: &[Stmt],
  output: &[Stmt],
  iterators: &[Iterator],
) -> AnchorSet {
  let mut a = AnchorSet::default();
  // Top-level iterators (empty parent) contribute up/down.
  for it in iterators {
    if it.parent.is_empty() {
      match it.direction {
        Direction::Up => a.up = true,
        Direction::Down => a.down = true,
      }
    }
  }
  // Statements whose path starts with `hex` or `root` contribute
  // those anchors. Inside-out chains (`up.slot.0.owner.hex…`) don't
  // count — only the FIRST segment of the statement's path matters
  // for top-level-anchor classification.
  for stmt in input.iter().chain(output.iter()) {
    if let Some(Seg::Word(w)) = stmt.segments.first() {
      match w.as_str() {
        "hex" => a.hex = true,
        "root" => a.root = true,
        _ => {}
      }
    }
  }
  a
}

/// Parse a whole recipe-file JSON object — flat top-level keyed by
/// recipe id, each value a `{ input, output }` object. Returns the
/// recipes in source order.
pub fn parse_file(filename: &str, content: &str) -> Result<Vec<Recipe>, String> {
  let parsed: Value = serde_json::from_str(content)
    .map_err(|e| format!("{filename}: parse failed: {e}"))?;
  let root = parsed.as_object().ok_or_else(|| {
    format!("{filename}: top-level must be an object keyed by recipe id")
  })?;
  let mut out = Vec::with_capacity(root.len());
  for (id, recipe_val) in root {
    out.push(parse_recipe(id, recipe_val).map_err(|e| format!("{filename}: {e}"))?);
  }
  Ok(out)
}

fn parse_one(s: &str, iterators: &mut Vec<Iterator>) -> Result<Stmt, String> {
  let raw = parse_statement(s)?;
  let segments = resolve_slots(&raw.path, iterators)?;
  Ok(Stmt {
    segments,
    value: raw.value,
  })
}

/// Walk raw segments left-to-right, collapsing every
/// `<up|down>.slot.<N>` triplet into a single [`Seg::Slot`] and
/// registering / reusing the corresponding [`Iterator`].
///
/// The iterator's parent path is captured from the already-resolved
/// output prefix at the moment of the pattern match — so iterators
/// can reference earlier iterators (via prior `Slot` entries in
/// `parent`) but never themselves or later ones.
fn resolve_slots(
  raw: &[Segment],
  iterators: &mut Vec<Iterator>,
) -> Result<Vec<Seg>, String> {
  let mut out: Vec<Seg> = Vec::with_capacity(raw.len());
  let mut i = 0;
  while i < raw.len() {
    // Pattern: <Word("up"|"down")> <Word("slot")> <Index(N)>
    if i + 2 < raw.len() {
      if let Some(direction) = direction_from_segment(&raw[i]) {
        if raw[i + 1].as_word() == Some("slot") {
          if let Some(offset) = raw[i + 2].as_index() {
            let parent = out.clone();
            let iterator_id = find_or_create_iterator(iterators, parent, direction);
            out.push(Seg::Slot {
              iterator_id,
              offset,
            });
            i += 3;
            continue;
          }
        }
      }
    }
    out.push(convert_segment(&raw[i]));
    i += 1;
  }
  Ok(out)
}

fn direction_from_segment(s: &Segment) -> Option<Direction> {
  match s.as_word()? {
    "up" => Some(Direction::Up),
    "down" => Some(Direction::Down),
    _ => None,
  }
}

fn convert_segment(s: &Segment) -> Seg {
  match s {
    Segment::Word(w) => Seg::Word(w.clone()),
    Segment::Index(n) => Seg::Index(*n),
  }
}

fn find_or_create_iterator(
  iterators: &mut Vec<Iterator>,
  parent: Vec<Seg>,
  direction: Direction,
) -> u32 {
  for (i, it) in iterators.iter().enumerate() {
    if it.parent == parent && it.direction == direction {
      return i as u32;
    }
  }
  let id = iterators.len() as u32;
  iterators.push(Iterator { parent, direction });
  id
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
  use super::*;

  fn w(s: &str) -> Seg {
    Seg::Word(s.to_string())
  }
  fn idx(n: u32) -> Seg {
    Seg::Index(n)
  }
  fn slot(iterator_id: u32, offset: u32) -> Seg {
    Seg::Slot {
      iterator_id,
      offset,
    }
  }

  fn parse(json: &str) -> Vec<Recipe> {
    parse_file("test.json", json).expect("parse")
  }

  #[test]
  fn parses_simple_flat_recipe() {
    let json = r#"{
      "triple_corpus": {
        "input": [
          "up.slot.0.def_id: corpus",
          "up.slot.1.def_id: corpus",
          "up.slot.2.def_id: corpus"
        ],
        "output": [
          "sys.duration.set: 10",
          "sys.style.set: rtl",
          "up.slot.0.destroy"
        ]
      }
    }"#;
    let recipes = parse(json);
    assert_eq!(recipes.len(), 1);
    let r = &recipes[0];
    assert_eq!(r.id, "triple_corpus");
    assert_eq!(r.input.len(), 3);
    assert_eq!(r.output.len(), 3);
    // All three input statements share one iterator over `up`.
    assert_eq!(r.iterators.len(), 1);
    assert_eq!(r.iterators[0].direction, Direction::Up);
    assert_eq!(r.iterators[0].parent, Vec::<Seg>::new());
    // First input: [Slot{0,0}, Word("def_id")]; value = "corpus".
    assert_eq!(r.input[0].segments, vec![slot(0, 0), w("def_id")]);
    assert_eq!(
      r.input[0].value,
      Some(StatementValue::Str("corpus".to_string()))
    );
    // Second input slot at offset 1.
    assert_eq!(r.input[1].segments, vec![slot(0, 1), w("def_id")]);
    // Output: sys.duration.set / sys.style.set don't touch slots.
    assert_eq!(
      r.output[0].segments,
      vec![w("sys"), w("duration"), w("set")]
    );
    assert_eq!(r.output[0].value, Some(StatementValue::Int(10)));
    // Destroy on slot.0.
    assert_eq!(r.output[2].segments, vec![slot(0, 0), w("destroy")]);
  }

  #[test]
  fn dedupes_up_slot_iterator_across_statements() {
    let json = r#"{
      "r": {
        "input": [
          "up.slot.0.def_id: A",
          "up.slot.1.def_id: B"
        ],
        "output": [
          "up.slot.0.destroy",
          "up.slot.1.destroy"
        ]
      }
    }"#;
    let recipes = parse(json);
    let r = &recipes[0];
    assert_eq!(r.iterators.len(), 1, "single up-iterator shared by all four refs");
  }

  #[test]
  fn up_and_down_are_distinct_iterators() {
    let json = r#"{
      "r": {
        "input": [
          "up.slot.0.def_id: A",
          "down.slot.0.def_id: B"
        ],
        "output": []
      }
    }"#;
    let recipes = parse(json);
    let r = &recipes[0];
    assert_eq!(r.iterators.len(), 2);
    assert_eq!(r.iterators[0].direction, Direction::Up);
    assert_eq!(r.iterators[1].direction, Direction::Down);
  }

  #[test]
  fn nested_iterator_for_equipment_chain() {
    // The cut_tree axe predicate: corpus+ in player's stack, axe in
    // the corpus+'s owner's equipment.
    let json = r#"{
      "cut_tree": {
        "input": [
          "hex.aspect.wood.min: 1",
          "up.slot.0.aspect.corpus+.min: 1",
          "up.slot.0.owner.up.slot.0.def_id: axe"
        ],
        "output": []
      }
    }"#;
    let recipes = parse(json);
    let r = &recipes[0];
    assert_eq!(
      r.iterators.len(),
      2,
      "top-level up + nested up via slot.0.owner"
    );
    // Iterator 0: top-level `up` with empty parent.
    assert_eq!(r.iterators[0].parent, Vec::<Seg>::new());
    assert_eq!(r.iterators[0].direction, Direction::Up);
    // Iterator 1: nested `up` whose parent is `[Slot{0,0}, Word("owner")]`.
    assert_eq!(
      r.iterators[1].parent,
      vec![slot(0, 0), w("owner")]
    );
    assert_eq!(r.iterators[1].direction, Direction::Up);
    // Verify the axe predicate's segments.
    assert_eq!(
      r.input[2].segments,
      vec![slot(0, 0), w("owner"), slot(1, 0), w("def_id")]
    );
    assert_eq!(
      r.input[2].value,
      Some(StatementValue::Str("axe".to_string()))
    );
  }

  #[test]
  fn parallel_nested_iterators_distinct_by_outer_offset() {
    // Two outer slots, each with their own nested equipment iterator.
    // The user's "bow + arrow" example: each outer slot has different
    // owner, so their equipment stacks are independent iterators.
    let json = r#"{
      "r": {
        "input": [
          "up.slot.0.def_id: A",
          "up.slot.1.def_id: B",
          "up.slot.0.owner.up.slot.0.def_id: C",
          "up.slot.0.owner.up.slot.1.def_id: D",
          "up.slot.1.owner.up.slot.0.def_id: E",
          "up.slot.1.owner.up.slot.1.def_id: F"
        ],
        "output": []
      }
    }"#;
    let recipes = parse(json);
    let r = &recipes[0];
    assert_eq!(
      r.iterators.len(),
      3,
      "outer up + two distinct nested ups (one per outer offset)"
    );
    // Iterator 0: outer up.
    assert_eq!(r.iterators[0].parent, Vec::<Seg>::new());
    // Iterator 1: nested up via slot.0.owner.
    assert_eq!(r.iterators[1].parent, vec![slot(0, 0), w("owner")]);
    // Iterator 2: nested up via slot.1.owner — distinct from iterator 1.
    assert_eq!(r.iterators[2].parent, vec![slot(0, 1), w("owner")]);
    // Predicates C and D share iterator 1.
    assert_eq!(
      r.input[2].segments,
      vec![slot(0, 0), w("owner"), slot(1, 0), w("def_id")]
    );
    assert_eq!(
      r.input[3].segments,
      vec![slot(0, 0), w("owner"), slot(1, 1), w("def_id")]
    );
    // E and F share iterator 2.
    assert_eq!(
      r.input[4].segments,
      vec![slot(0, 1), w("owner"), slot(2, 0), w("def_id")]
    );
    assert_eq!(
      r.input[5].segments,
      vec![slot(0, 1), w("owner"), slot(2, 1), w("def_id")]
    );
  }

  #[test]
  fn variable_and_when_segments_preserved_as_words() {
    // Variable computation + when-gated overwrite (fleeting recipe).
    // `var.0.set` with a string value carries a path the executor reads
    // at runtime — same op as a literal `var.0.set: 5`, dispatch
    // happens on the value's type (Int vs Str).
    let json = r#"{
      "fleeting": {
        "input": [
          "root.aspect.fleeting.min: 1"
        ],
        "output": [
          "var.0.set: root.aspect.fleeting",
          "sys.duration.set: 5",
          "when.var.0.ge.2.sys.duration.set: 10",
          "root.destroy"
        ]
      }
    }"#;
    let recipes = parse(json);
    let r = &recipes[0];
    // No slot refs in this recipe → no iterators.
    assert_eq!(r.iterators.len(), 0);
    // var.0.set: <path-string>; executor reads RHS as path.
    assert_eq!(
      r.output[0].segments,
      vec![w("var"), idx(0), w("set")]
    );
    assert_eq!(
      r.output[0].value,
      Some(StatementValue::Str("root.aspect.fleeting".to_string()))
    );
    // when.X.ge.2.sys.duration.set: 10 — whole prefix preserved.
    assert_eq!(
      r.output[2].segments,
      vec![
        w("when"),
        w("var"),
        idx(0),
        w("ge"),
        idx(2),
        w("sys"),
        w("duration"),
        w("set"),
      ]
    );
    assert_eq!(r.output[2].value, Some(StatementValue::Int(10)));
  }

  #[test]
  fn create_with_owner_chain_value() {
    // Result lands in slot owner's inventory — path-first verb at end.
    let json = r#"{
      "corpus_up": {
        "input": [
          "up.slot.0.def_id: corpus",
          "up.slot.1.def_id: corpus"
        ],
        "output": [
          "up.slot.0.destroy",
          "up.slot.0.owner.inventory.create: corpus-"
        ]
      }
    }"#;
    let recipes = parse(json);
    let r = &recipes[0];
    assert_eq!(
      r.output[1].segments,
      vec![slot(0, 0), w("owner"), w("inventory"), w("create")]
    );
    assert_eq!(
      r.output[1].value,
      Some(StatementValue::Str("corpus-".to_string()))
    );
  }

  #[test]
  fn missing_input_or_output_errors() {
    assert!(parse_file("t", r#"{"r":{"output":[]}}"#).is_err());
    assert!(parse_file("t", r#"{"r":{"input":[]}}"#).is_err());
  }

  #[test]
  fn non_string_statement_errors() {
    assert!(parse_file("t", r#"{"r":{"input":[42],"output":[]}}"#).is_err());
  }

  #[test]
  fn malformed_statement_propagates_error() {
    // `Slot` and ` ` aren't legal — uppercase + whitespace.
    let bad = r#"{"r":{"input":["up.Slot.0.def_id: X"],"output":[]}}"#;
    let err = parse_file("t", bad).unwrap_err();
    assert!(err.contains("input[0]"), "error preserves position: {err}");
  }

  // ---- AnchorSet classification ----------------------------------------

  #[test]
  fn anchors_top_level_up_iterator() {
    let json = r#"{
      "r": { "input": ["up.slot.0.def_id: corpus"], "output": [] }
    }"#;
    let r = &parse(json)[0];
    assert!(r.anchors.up && !r.anchors.down && !r.anchors.hex && !r.anchors.root);
  }

  #[test]
  fn anchors_hex_and_root() {
    let json = r#"{
      "r": {
        "input": [
          "hex.aspect.wood.min: 1",
          "root.def_id: foo"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert!(r.anchors.hex && r.anchors.root && !r.anchors.up && !r.anchors.down);
  }

  #[test]
  fn anchors_nested_iterator_does_not_count() {
    // Nested iterator (parent != empty) does NOT register `up` at the
    // top-level set — it's parameterized on an outer slot binding.
    let json = r#"{
      "r": {
        "input": [
          "root.def_id: x",
          "root.owner.up.slot.0.def_id: axe"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert!(r.anchors.root, "root anchor present");
    assert!(!r.anchors.up, "nested up (via root.owner) is not a top-level anchor");
  }

  #[test]
  fn priority_key_orders_longest_first() {
    // {hex,root,up,down} > {hex,root,up} > … > {hex} > {} etc.
    let a_all = AnchorSet { hex: true, root: true, up: true, down: true };
    let a_hex_root_up = AnchorSet { hex: true, root: true, up: true, down: false };
    let a_hex_root_down = AnchorSet { hex: true, root: true, up: false, down: true };
    let a_hex_up_down = AnchorSet { hex: true, root: false, up: true, down: true };
    let a_root_up_down = AnchorSet { hex: false, root: true, up: true, down: true };
    let a_hex = AnchorSet { hex: true, root: false, up: false, down: false };

    assert!(a_all.priority_key() > a_hex_root_up.priority_key());
    assert!(a_hex_root_up.priority_key() > a_hex_root_down.priority_key());
    assert!(a_hex_root_down.priority_key() > a_hex_up_down.priority_key());
    assert!(a_hex_up_down.priority_key() > a_root_up_down.priority_key());
    // Longer wins over higher-priority short — {root,up,down} > {hex}.
    assert!(a_root_up_down.priority_key() > a_hex.priority_key());
  }

  #[test]
  fn anchorset_subset_check() {
    let available = AnchorSet { hex: true, root: true, up: true, down: false };
    let need_hex_root = AnchorSet { hex: true, root: true, up: false, down: false };
    let need_up = AnchorSet { hex: false, root: false, up: true, down: false };
    let need_down = AnchorSet { hex: false, root: false, up: false, down: true };
    assert!(need_hex_root.is_subset_of(&available));
    assert!(need_up.is_subset_of(&available));
    assert!(!need_down.is_subset_of(&available), "down not available");
  }
}
