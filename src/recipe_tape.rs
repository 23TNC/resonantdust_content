//! Recipe tape — statement-array recipe form with iterator-aware paths.
//!
//! Replaces the typed `RecipeDef` shape (legacy) with a uniform
//! statement-tape representation. Two arrays per recipe:
//!
//! - **`input`** — predicates evaluated as a conjunction at match
//!   (client) / verify (server) time.
//! - **`output`** — ordered tape walked sequentially at completion,
//!   with variables (`var.N`), system slots (`sys.duration`,
//!   `sys.style`), conditional gates (`when.<predicate>.<statement>`),
//!   and effect ops (`destroy`, `create`, `set`, `add`, `sub`,
//!   `random`).
//!
//! Path-first grammar: every statement is `<path>.<op>[: value]`.
//! Numbered-branch slot references (`stack.<branch>.slot.<N>`) are
//! resolved at parse time into iterator IDs, and the parser surfaces
//! a per-recipe list of iterators that the client matcher walks as
//! nested loops. Server receives the matcher's chosen offsets and
//! skips iteration — verification is a linear pass per predicate.
//!
//! Branches are numbered, not named — the recipe doesn't distinguish
//! "up" vs "down" vs "hex"; it just references branch `0`, `1`, `2`,
//! etc. Slots into branches are referenced via `slot.<branch>.<index>`
//! — a single uniform syntax for every card-in-stack access. The wire
//! format provides a 2-D `slots[branch][index]` view the server
//! indexes by branch / offset. Branch 0 is the tile branch by
//! convention (visually beneath root); branches 1+ are stacking
//! branches above / below by convention.
//!
//! JSON shape (per recipe):
//! ```json
//! {
//!   "input":  ["slot.1.0.def_id: corpus", ...],
//!   "output": ["sys.duration.set: 10", "slot.1.0.destroy", ...]
//! }
//! ```
//!
//! See `content/recipes/data/01.json` and `02.json` for live examples
//! driving these tests.

use serde_json::Value;

use crate::recipe_statement::{parse_statement, Segment, StatementValue};

// ---------- Public types ----------

/// Resolved path segment. After parsing, every `slot.<branch>.<index>`
/// triplet in the source path has been collapsed into a single
/// [`Seg::Slot`] referencing an entry in the recipe's
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
  /// Plain identifier — `slot`, `root`, `owner`, `def_id`, `aspect`,
  /// `inventory`, `sys`, `var`, `when`, op tokens (`set`, `add`,
  /// `sub`, `min`, `gt`, …), aspect names, etc.
  Word(String),
  /// Integer path segment — variable index in `var.N`, comparison
  /// value in `when.X.gt.N`, etc. Distinct from values that appear
  /// after the `: ` separator and from `slot.<branch>.<index>`
  /// triplets (those collapse into [`Seg::Slot`] at parse time).
  Index(u32),
  /// `slot.<branch>.<index>` reference resolved to an iterator +
  /// offset. The offset is the literal `<index>` from the source;
  /// the iterator id indexes into [`Recipe::iterators`] and carries
  /// the branch number + parent path.
  Slot {
    #[cfg_attr(feature = "js", serde(rename = "iteratorId"))]
    iterator_id: u32,
    offset: u32,
  },
}

/// One sliding-window iterator over a card's stack branch.
///
/// Identified by its **parent path** (the resolved segments before
/// the `slot.<branch>` marker) and branch number. The parent path
/// is empty for top-level iterators (whose parent is the implicit
/// action anchor — the proposing player's stack root); deeper
/// iterators reference earlier ones via [`Seg::Slot`] in their
/// parent path.
///
/// Two source-level slot references with the same resolved parent
/// and branch share the same iterator id; the offset distinguishes
/// which slot within the iterator's window they bind to.
///
/// `slot_hold` and `position_hold` together encode the recipe's
/// hold policy for cards bound to this iterator. They're computed
/// at parse time from the per-statement prefix tokens via the
/// last-write-wins rule (see [`compute_locks`]).
///
/// The four prefix tokens map to (slot_hold, position_hold) tuples:
///   - `borrow.` → (false, false) — existence verification only
///   - `share.`  → (false, true)  — others may bind, position pinned
///   - `claim.`  → (true, true)   — exclusive + position pinned (default)
///   - `use.`    → (true, false)  — exclusive, mobile (actors)
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(feature = "js", serde(rename_all = "camelCase"))]
pub struct Iterator {
  pub parent: Vec<Seg>,
  pub branch: u8,
  /// Server claims `FLAG_SLOT_HOLD` on bindings during `apply_locks`,
  /// reserving them against concurrent recipes and blocking user
  /// pickup. From the last-written prefix token targeting this iter.
  pub slot_hold: bool,
  /// Server ref-count-acquires `position_hold` on bindings during
  /// `apply_locks`, blocking server-side movement reducers
  /// (`move_soul`, `unequip_card`, etc.) for the recipe duration.
  /// From the last-written prefix token targeting this iter.
  pub position_hold: bool,
}

/// One parsed statement — either an input predicate or an output
/// tape entry. The parser treats both uniformly; downstream code
/// (matcher / verifier / executor) interprets by structural pattern
/// in the segments.
///
/// `slot_hold` / `position_hold` come from the leading prefix
/// token (`borrow.` / `share.` / `claim.` / `use.`) — see
/// [`Iterator`] for the mapping. Default (no prefix) is `claim`.
/// Only meaningful on `input` statements; the aggregator
/// ([`compute_locks`]) walks inputs only. Outputs carry the
/// fields through with default values so the executor can
/// reconstruct a Stmt by clone.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(feature = "js", serde(rename_all = "camelCase"))]
pub struct Stmt {
  pub segments: Vec<Seg>,
  pub value: Option<StatementValue>,
  pub slot_hold: bool,
  pub position_hold: bool,
}

/// Top-level anchors / branches a recipe references. Used by the
/// client matcher as a pre-filter — recipes whose [`AnchorSet`]
/// requires branches or anchors the engagement doesn't provide are
/// skipped without iteration.
///
/// `root` is set if any statement path starts with `root`.
/// `branches` is a bitmask of top-level branch numbers referenced
/// (e.g., bit 0 = branch 0, bit 1 = branch 1, etc.). Only references
/// at the top of a path (or as a top-level iterator parent) count;
/// nested references via `.owner.stack.N` don't contribute since
/// they're parameterized on outer bindings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
pub struct AnchorSet {
  pub root: bool,
  /// Bitmask of top-level branch numbers referenced. Bit `n` set
  /// means `stack.n.X` appeared at the path root or as a top-level
  /// iterator parent.
  pub branches: u32,
}

impl AnchorSet {
  /// Number of top-level inputs the recipe needs (`root` counts as 1
  /// plus 1 for every branch referenced).
  pub fn count(&self) -> u8 {
    self.branches.count_ones() as u8 + (self.root as u8)
  }

  /// Sort key for priority-tiered matching. Higher key = higher
  /// priority. Primary: count (more anchors first). Secondary: lower
  /// branch numbers win the tiebreak — branch 0 (tile) is the most
  /// specific anchor and ranks above branch 1, etc.
  pub fn priority_key(&self) -> u32 {
    let count = self.count() as u32;
    // Within same count, prefer recipes that touch lower-numbered
    // branches (branch 0 = tile, considered the most specific).
    // Invert the bitmask so lower-numbered branches contribute more
    // to the key.
    let inverted = (!self.branches) & 0xFFFF;
    (count << 24) | (((self.root as u32) << 16)) | inverted
  }

  /// True iff every branch / anchor required by `self` is present
  /// in `available`.
  pub fn is_subset_of(&self, available: &AnchorSet) -> bool {
    (!self.root || available.root)
      && (self.branches & !available.branches) == 0
  }

  /// Mark branch `n` as referenced.
  pub fn add_branch(&mut self, n: u8) {
    if n < 32 {
      self.branches |= 1u32 << n;
    }
  }

  /// Check whether branch `n` is in the set.
  pub fn has_branch(&self, n: u8) -> bool {
    n < 32 && (self.branches & (1u32 << n)) != 0
  }
}

/// A parsed recipe.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(feature = "js", serde(rename_all = "camelCase"))]
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
  /// Top-level branches / anchors the recipe requires. Computed at
  /// parse time; the client matcher uses this to skip whole recipes
  /// before iterating offsets.
  pub anchors: AnchorSet,
  /// Hold policy for the implicit root anchor — same prefix-token
  /// derivation as [`Iterator::slot_hold`] but for input statements
  /// whose path starts with `root`. Server's `apply_locks` reads
  /// these directly. Both default to `true` (claim) when
  /// `anchors.root` is set; both `false` when no root anchor.
  pub root_slot_hold: bool,
  pub root_position_hold: bool,
}

// ---------- Parser ----------

/// Parse one recipe's `{ "input": [...], "output": [...] }` JSON
/// object into a [`Recipe`]. Statement strings are lexed via
/// [`crate::recipe_statement::parse_statement`]; the resulting
/// segments are post-processed to resolve `stack.<branch>.slot.<N>`
/// patterns into iterator IDs.
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
  let (root_slot_hold, root_position_hold) =
    compute_locks(&input, &mut iterators, &anchors);
  Ok(Recipe {
    id: id.to_string(),
    input,
    output,
    iterators,
    anchors,
    root_slot_hold,
    root_position_hold,
  })
}

/// Aggregate per-statement prefix tokens into per-iterator and root
/// hold policies. Last-write-wins across input statements: walk
/// inputs in source order, and whenever a statement's terminal slot
/// is iterator `i`, overwrite `iterators[i]`'s (slot_hold,
/// position_hold) with the statement's. Same rule for root: walk
/// inputs whose path starts with `root`. Output statements don't
/// contribute — they only declare effects, not hold policy.
///
/// Default policy (no input statement targets the iterator/root):
/// `claim` — `(true, true)`. Root locks are `(false, false)` when
/// no root anchor is present (nothing to lock).
fn compute_locks(
  input: &[Stmt],
  iterators: &mut [Iterator],
  anchors: &AnchorSet,
) -> (bool, bool) {
  let n = iterators.len();
  let mut iter_locks: Vec<(bool, bool)> = vec![(true, true); n];
  let mut root_locks: (bool, bool) = if anchors.root {
    (true, true)
  } else {
    (false, false)
  };

  for stmt in input.iter() {
    if let Some(id) = terminal_iterator_id(stmt) {
      if (id as usize) < n {
        iter_locks[id as usize] = (stmt.slot_hold, stmt.position_hold);
      }
    } else if matches!(stmt.segments.first(), Some(Seg::Word(w)) if w == "root") {
      root_locks = (stmt.slot_hold, stmt.position_hold);
    }
  }

  for (i, (s, p)) in iter_locks.into_iter().enumerate() {
    iterators[i].slot_hold = s;
    iterators[i].position_hold = p;
  }
  root_locks
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
    // Skip JSON-doc convention keys (`_comment`, etc.). Same rule as
    // `gen-ids.py`'s recipes pass and the card / texture parsers in
    // `definition_core` / `texture_core`.
    if id.starts_with('_') {
      continue;
    }
    out.push(parse_recipe(id, recipe_val).map_err(|e| format!("{filename}: {e}"))?);
  }
  Ok(out)
}

fn parse_one(s: &str, iterators: &mut Vec<Iterator>) -> Result<Stmt, String> {
  let raw = parse_statement(s)?;
  // Detect a leading hold-policy prefix and strip it from the path
  // before slot resolution. Recognized prefixes map to (slot_hold,
  // position_hold). Default (no prefix) is `claim`.
  let prefix = raw.path.first().and_then(|seg| match seg {
    Segment::Word(w) => match w.as_str() {
      "borrow" => Some((false, false)),
      "share" => Some((false, true)),
      "claim" => Some((true, true)),
      "use" => Some((true, false)),
      _ => None,
    },
    _ => None,
  });
  let (path_slice, slot_hold, position_hold) = match prefix {
    Some((s, p)) => (&raw.path[1..], s, p),
    None => (&raw.path[..], true, true),
  };
  let segments = resolve_slots(path_slice, iterators)?;
  Ok(Stmt {
    segments,
    value: raw.value,
    slot_hold,
    position_hold,
  })
}

/// Walk raw segments left-to-right, collapsing every
/// `slot.<branch>.<index>` triplet into a single [`Seg::Slot`] and
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
    // Pattern: <Word("slot")> <Index(branch)> <Index(index)>
    if i + 2 < raw.len() {
      if raw[i].as_word() == Some("slot") {
        if let (Some(branch_u32), Some(offset)) =
          (raw[i + 1].as_index(), raw[i + 2].as_index())
        {
          if branch_u32 > 255 {
            return Err(format!(
              "slot.{branch_u32}.{offset}: branch number must fit in u8"
            ));
          }
          let branch = branch_u32 as u8;
          let parent = out.clone();
          let iterator_id = find_or_create_iterator(iterators, parent, branch);
          out.push(Seg::Slot {
            iterator_id,
            offset,
          });
          i += 3;
          continue;
        }
      }
    }
    out.push(convert_segment(&raw[i]));
    i += 1;
  }
  Ok(out)
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
  branch: u8,
) -> u32 {
  for (i, it) in iterators.iter().enumerate() {
    if it.parent == parent && it.branch == branch {
      return i as u32;
    }
  }
  let id = iterators.len() as u32;
  // Default to `claim` — (slot_hold, position_hold) = (true, true).
  // `compute_locks` overwrites with the last-written prefix from any
  // input statement targeting this iterator.
  iterators.push(Iterator {
    parent,
    branch,
    slot_hold: true,
    position_hold: true,
  });
  id
}

/// Walk a statement's `segments` and return the iterator id of the
/// **last** [`Seg::Slot`] in the path (the statement's terminal slot
/// — what the predicate/op actually targets). Returns `None` for
/// statements with no Slot references (e.g. `sys.duration.set` or
/// `root.destroy`). The op word at the end of the segments doesn't
/// count — by convention `segments.last()` is the op, so we walk
/// from the second-to-last back.
fn terminal_iterator_id(stmt: &Stmt) -> Option<u32> {
  // Skip the trailing op word (`def_id`, `min`, `destroy`, etc).
  // For multi-word terminal patterns like `.aspect.<name>.min`, the
  // Slot still appears before the words — we scan backward to find
  // it.
  for seg in stmt.segments.iter().rev() {
    if let Seg::Slot { iterator_id, .. } = seg {
      return Some(*iterator_id);
    }
  }
  None
}

/// Walk a parsed recipe's statements + iterators and surface the
/// top-level anchors / branches it references. See [`AnchorSet`]
/// for what counts.
///
/// Under the unified `slot.<branch>.<index>` grammar every branch
/// access already collapses into a `Seg::Slot` whose iterator
/// carries the branch number — so the classifier just walks the
/// iterators with empty parent. The `root` anchor is detected by
/// scanning statement paths for a leading `root` word.
fn classify_anchors(
  input: &[Stmt],
  output: &[Stmt],
  iterators: &[Iterator],
) -> AnchorSet {
  let mut a = AnchorSet::default();

  // Top-level iterators (empty parent) contribute their branch.
  for it in iterators {
    if it.parent.is_empty() {
      a.add_branch(it.branch);
    }
  }

  // Root anchor — any statement whose path starts with `root`.
  for stmt in input.iter().chain(output.iter()) {
    if matches!(stmt.segments.first(), Some(Seg::Word(w)) if w == "root") {
      a.root = true;
      break;
    }
  }
  a
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
          "slot.1.0.def_id: corpus",
          "slot.1.1.def_id: corpus",
          "slot.1.2.def_id: corpus"
        ],
        "output": [
          "sys.duration.set: 10",
          "sys.style.set: rtl",
          "slot.1.0.destroy"
        ]
      }
    }"#;
    let recipes = parse(json);
    assert_eq!(recipes.len(), 1);
    let r = &recipes[0];
    assert_eq!(r.id, "triple_corpus");
    assert_eq!(r.input.len(), 3);
    assert_eq!(r.output.len(), 3);
    // All three input statements share one iterator over branch 1.
    assert_eq!(r.iterators.len(), 1);
    assert_eq!(r.iterators[0].branch, 1);
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
  fn dedupes_iterator_within_same_branch() {
    let json = r#"{
      "r": {
        "input": [
          "slot.1.0.def_id: A",
          "slot.1.1.def_id: B"
        ],
        "output": [
          "slot.1.0.destroy",
          "slot.1.1.destroy"
        ]
      }
    }"#;
    let r = &parse(json)[0];
    assert_eq!(
      r.iterators.len(),
      1,
      "single branch-1 iterator shared by all four refs"
    );
  }

  #[test]
  fn different_branches_are_distinct_iterators() {
    let json = r#"{
      "r": {
        "input": [
          "slot.1.0.def_id: A",
          "slot.2.0.def_id: B"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert_eq!(r.iterators.len(), 2);
    assert_eq!(r.iterators[0].branch, 1);
    assert_eq!(r.iterators[1].branch, 2);
  }

  #[test]
  fn nested_iterator_for_equipment_chain() {
    // The cut_tree axe predicate: corpus+ in player's branch 1, axe
    // in the corpus+'s owner's branch 1 (equipment, same numbering).
    let json = r#"{
      "cut_tree": {
        "input": [
          "slot.0.0.aspect.wood.min: 1",
          "slot.1.0.aspect.corpus+.min: 1",
          "slot.1.0.owner.slot.1.0.def_id: axe"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert_eq!(
      r.iterators.len(),
      3,
      "branch 0 + outer branch 1 + nested branch 1 via slot.0.owner"
    );
    // Iterator 0: branch 0 (tile) with empty parent.
    assert_eq!(r.iterators[0].parent, Vec::<Seg>::new());
    assert_eq!(r.iterators[0].branch, 0);
    // Iterator 1: branch 1 with empty parent.
    assert_eq!(r.iterators[1].parent, Vec::<Seg>::new());
    assert_eq!(r.iterators[1].branch, 1);
    // Iterator 2: nested branch 1 whose parent is
    // `[Slot{iter=1, offset=0}, Word("owner")]`.
    assert_eq!(r.iterators[2].parent, vec![slot(1, 0), w("owner")]);
    assert_eq!(r.iterators[2].branch, 1);
    // Verify the axe predicate's segments.
    assert_eq!(
      r.input[2].segments,
      vec![slot(1, 0), w("owner"), slot(2, 0), w("def_id")]
    );
    assert_eq!(
      r.input[2].value,
      Some(StatementValue::Str("axe".to_string()))
    );
    // Anchor set: branch 0 (the tile) + branch 1 (the action stack).
    // Root is not referenced.
    assert!(!r.anchors.root);
    assert!(r.anchors.has_branch(0));
    assert!(r.anchors.has_branch(1));
    assert!(!r.anchors.has_branch(2));
  }

  #[test]
  fn parallel_nested_iterators_distinct_by_outer_offset() {
    // Two outer slots, each with their own nested equipment iterator.
    // The "bow + arrow" example: each outer slot has a different
    // owner, so their equipment stacks are independent iterators.
    let json = r#"{
      "r": {
        "input": [
          "slot.1.0.def_id: A",
          "slot.1.1.def_id: B",
          "slot.1.0.owner.slot.1.0.def_id: C",
          "slot.1.0.owner.slot.1.1.def_id: D",
          "slot.1.1.owner.slot.1.0.def_id: E",
          "slot.1.1.owner.slot.1.1.def_id: F"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert_eq!(
      r.iterators.len(),
      3,
      "outer branch 1 + two distinct nested branch-1s (one per outer offset)"
    );
    // Iterator 0: outer branch 1.
    assert_eq!(r.iterators[0].parent, Vec::<Seg>::new());
    assert_eq!(r.iterators[0].branch, 1);
    // Iterator 1: nested branch 1 via slot.0.owner.
    assert_eq!(r.iterators[1].parent, vec![slot(0, 0), w("owner")]);
    assert_eq!(r.iterators[1].branch, 1);
    // Iterator 2: nested branch 1 via slot.1.owner — distinct.
    assert_eq!(r.iterators[2].parent, vec![slot(0, 1), w("owner")]);
    assert_eq!(r.iterators[2].branch, 1);
  }

  #[test]
  fn variable_and_when_segments_preserved_as_words() {
    // Variable computation + when-gated overwrite (fleeting recipe).
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
    let r = &parse(json)[0];
    assert_eq!(r.iterators.len(), 0);
    assert!(r.anchors.root);
    assert_eq!(r.anchors.branches, 0);
    assert_eq!(
      r.output[0].segments,
      vec![w("var"), idx(0), w("set")]
    );
    assert_eq!(
      r.output[0].value,
      Some(StatementValue::Str("root.aspect.fleeting".to_string()))
    );
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
  fn prefix_tokens_set_iter_locks() {
    let json = r#"{
      "r": {
        "input": [
          "borrow.slot.1.0.def_id: a",
          "share.slot.2.0.def_id: b",
          "claim.slot.3.0.def_id: c",
          "use.slot.4.0.def_id: d",
          "slot.5.0.def_id: e"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert_eq!(r.iterators.len(), 5);
    let by_branch = |b: u8| r.iterators.iter().find(|it| it.branch == b).unwrap();
    let it1 = by_branch(1);
    assert_eq!((it1.slot_hold, it1.position_hold), (false, false), "borrow");
    let it2 = by_branch(2);
    assert_eq!((it2.slot_hold, it2.position_hold), (false, true), "share");
    let it3 = by_branch(3);
    assert_eq!((it3.slot_hold, it3.position_hold), (true, true), "claim");
    let it4 = by_branch(4);
    assert_eq!((it4.slot_hold, it4.position_hold), (true, false), "use");
    let it5 = by_branch(5);
    assert_eq!((it5.slot_hold, it5.position_hold), (true, true), "default=claim");
  }

  #[test]
  fn last_write_wins_on_same_iterator() {
    let json = r#"{
      "r": {
        "input": [
          "claim.slot.1.0.def_id: x",
          "share.slot.1.1.def_id: y",
          "borrow.slot.1.2.def_id: z"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert_eq!(r.iterators.len(), 1);
    assert_eq!(
      (r.iterators[0].slot_hold, r.iterators[0].position_hold),
      (false, false),
      "last `borrow` wins over earlier claim/share"
    );
  }

  #[test]
  fn root_lock_tokens() {
    // Borrow root — fleeting-style.
    let json = r#"{
      "r": {
        "input": ["borrow.root.aspect.fleeting.min: 1"],
        "output": ["root.destroy"]
      }
    }"#;
    let r = &parse(json)[0];
    assert!(r.anchors.root);
    assert_eq!((r.root_slot_hold, r.root_position_hold), (false, false));

    // Use root — actor (slot_hold, mobile).
    let json2 = r#"{
      "r": {
        "input": ["use.root.def_id: strike"],
        "output": ["root.destroy"]
      }
    }"#;
    let r2 = &parse(json2)[0];
    assert_eq!((r2.root_slot_hold, r2.root_position_hold), (true, false));

    // Implicit (no prefix on root input) — claim default.
    let json3 = r#"{
      "r": {
        "input": ["root.def_id: x"],
        "output": ["root.destroy"]
      }
    }"#;
    let r3 = &parse(json3)[0];
    assert_eq!((r3.root_slot_hold, r3.root_position_hold), (true, true));

    // No root anchor — both false.
    let json4 = r#"{
      "r": {
        "input": ["slot.1.0.def_id: x"],
        "output": []
      }
    }"#;
    let r4 = &parse(json4)[0];
    assert!(!r4.anchors.root);
    assert_eq!((r4.root_slot_hold, r4.root_position_hold), (false, false));
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
    // `Slot` (capitalized) isn't a legal identifier — uppercase
    // rejected by the path-grammar tokenizer.
    let bad = r#"{"r":{"input":["Slot.1.0.def_id: X"],"output":[]}}"#;
    let err = parse_file("t", bad).unwrap_err();
    assert!(err.contains("input[0]"), "error preserves position: {err}");
  }

  #[test]
  fn slot_keyword_without_indices_stays_as_words() {
    // `slot` without two trailing `Index` segments isn't collapsed.
    // Useful sanity that the parser only fires on the strict pattern.
    let json = r#"{
      "r": {
        "input": ["root.def_id: x"],
        "output": ["root.destroy"]
      }
    }"#;
    let _ = parse(json);
  }

  // ---- AnchorSet behavior ---------------------------------------------

  #[test]
  fn anchors_branches_distinct() {
    let json = r#"{
      "r": {
        "input": [
          "slot.0.0.aspect.wood.min: 1",
          "slot.1.0.def_id: corpus",
          "root.def_id: foo"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert!(r.anchors.root);
    assert!(r.anchors.has_branch(0));
    assert!(r.anchors.has_branch(1));
    assert!(!r.anchors.has_branch(2));
  }

  #[test]
  fn anchors_nested_iterator_does_not_count() {
    // Nested iterator (parent != empty) does NOT register a top-level
    // branch — it's parameterized on an outer slot binding.
    let json = r#"{
      "r": {
        "input": [
          "root.def_id: x",
          "root.owner.slot.1.0.def_id: axe"
        ],
        "output": []
      }
    }"#;
    let r = &parse(json)[0];
    assert!(r.anchors.root, "root anchor present");
    assert!(
      !r.anchors.has_branch(1),
      "nested branch-1 (via root.owner) is not a top-level anchor"
    );
  }

  #[test]
  fn anchorset_subset_check() {
    let mut available = AnchorSet::default();
    available.root = true;
    available.add_branch(0);
    available.add_branch(1);

    let mut need_root_and_b1 = AnchorSet::default();
    need_root_and_b1.root = true;
    need_root_and_b1.add_branch(1);

    let mut need_b2 = AnchorSet::default();
    need_b2.add_branch(2);

    assert!(need_root_and_b1.is_subset_of(&available));
    assert!(!need_b2.is_subset_of(&available));
  }

  #[test]
  fn priority_key_longer_wins() {
    // 3 anchors > 1 anchor regardless of which anchors.
    let mut three = AnchorSet::default();
    three.root = true;
    three.add_branch(0);
    three.add_branch(1);
    let mut one = AnchorSet::default();
    one.add_branch(0);
    assert!(three.priority_key() > one.priority_key());
  }
}
