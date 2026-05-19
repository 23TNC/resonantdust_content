//! Statement-array recipe grammar — parser foundations.
//!
//! Recipes are JSON arrays of statement strings; each string is one
//! of:
//!
//! - `"<path>"` — verb-only (e.g. `output.destroy.slot.0`).
//! - `"<path>: <value>"` — path-plus-scalar (e.g.
//!   `input.hex.aspect.wood.min: 1`, `style.default: rtl`).
//!
//! A **path** is a dot-separated list of [`Segment`]s. Each segment
//! is either a [`Word`] (identifier) or an [`Index`] (integer —
//! used for `slot.<N>` references). The first segment is the
//! top-level bucket (`input` / `output` / `duration` / `style`); the
//! per-bucket grammar arms in higher-level parsers walk the
//! remainder.
//!
//! This module ships only the foundations:
//!
//! - [`Statement`] / [`Segment`] / [`StatementValue`] types.
//! - [`parse_statement`] — one-string-in, statement-out.
//! - [`is_reserved_aspect_name`] — closed-set check used to reject
//!   aspect names that would collide with path-grammar tokens.
//!
//! Recipe-level parsing — input/output statement arrays, iterator
//! resolution, anchor classification — lives in [`crate::recipe_tape`].

/// One segment of a dotted statement path. `Word("aspect")` for a
/// plain identifier, `Index(0)` for the `0` in `slot.0`. Integer
/// segments are only legal where the previous segment expects an
/// index (today: after `slot`); other contexts treat them as a
/// parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
  Word(String),
  Index(u32),
}

impl Segment {
  /// Borrow the word, if this segment is a word. `None` for `Index`.
  pub fn as_word(&self) -> Option<&str> {
    match self {
      Segment::Word(s) => Some(s.as_str()),
      Segment::Index(_) => None,
    }
  }

  pub fn as_index(&self) -> Option<u32> {
    match self {
      Segment::Index(i) => Some(*i),
      Segment::Word(_) => None,
    }
  }
}

/// Scalar value carried by a statement. Strings appear after a `: `
/// separator and are stored verbatim (trimmed of the surrounding
/// whitespace); integers are parsed eagerly so per-bucket grammars
/// see typed numbers.
///
/// Serialized to JS untagged — Int becomes a number, Str becomes a
/// string. Matches JSON-native typing so the client doesn't need to
/// discriminate on a tag field.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(feature = "js", serde(untagged))]
pub enum StatementValue {
  Int(i64),
  Str(String),
}

impl StatementValue {
  pub fn as_int(&self) -> Option<i64> {
    match self {
      StatementValue::Int(n) => Some(*n),
      StatementValue::Str(_) => None,
    }
  }

  pub fn as_str(&self) -> Option<&str> {
    match self {
      StatementValue::Str(s) => Some(s.as_str()),
      StatementValue::Int(_) => None,
    }
  }
}

/// One parsed statement — a path plus an optional scalar value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
  pub path: Vec<Segment>,
  pub value: Option<StatementValue>,
}

impl Statement {
  /// First-segment dispatch helper. Returns `None` for an empty
  /// path (shouldn't happen post-parse) or when the first segment
  /// is an integer (also rejected by [`parse_statement`]).
  pub fn bucket(&self) -> Option<&str> {
    self.path.first().and_then(Segment::as_word)
  }
}

// ---------- Statement parser ---------------------------------------------

/// Parse one statement string into a [`Statement`]. Errors carry
/// the offending input verbatim for author-side diagnostics.
///
/// Form:
///   `<path>` | `<path>: <value>`
///
/// `<value>` is matched as an integer first (signed-int parse over
/// the trimmed tail); on parse failure the tail becomes a string
/// value. Empty tails (`"x: "`) are a parse error.
///
/// `<path>` is split on `.`. Each segment is either a word
/// (`[a-z0-9_+-]+`) or a decimal integer (parsed via
/// `str::parse::<u32>`). `+` / `-` are accepted because stat-modifier
/// aspect names (`corpus+`, `corpus-`, `corpus++`) flow into path
/// segments via `aspect.<name>.min`-style predicates.
///
/// Whitespace is illegal inside the path; the `: ` separator is the
/// only place it's allowed.
pub fn parse_statement(s: &str) -> Result<Statement, String> {
  let trimmed = s.trim();
  if trimmed.is_empty() {
    return Err("empty statement".to_string());
  }

  // Split on the first ": " (colon + space). Bare `:` without a
  // following space is treated as path content — keeps the door
  // open for future colon-bearing tokens without breaking the
  // separator.
  let (path_str, value): (&str, Option<StatementValue>) = match trimmed.find(": ") {
    Some(idx) => {
      let (lhs, rhs) = trimmed.split_at(idx);
      let value_str = rhs[2..].trim();
      if value_str.is_empty() {
        return Err(format!("statement {trimmed:?}: empty value after ': '"));
      }
      let value = parse_statement_value(value_str);
      (lhs.trim_end(), Some(value))
    }
    None => (trimmed, None),
  };

  let path = parse_path(path_str)
    .map_err(|e| format!("statement {trimmed:?}: {e}"))?;
  if path.is_empty() {
    return Err(format!("statement {trimmed:?}: empty path"));
  }
  // First segment must be a bucket word — integers are never a
  // bucket name.
  if path.first().and_then(Segment::as_word).is_none() {
    return Err(format!(
      "statement {trimmed:?}: first segment must be a word, got integer"
    ));
  }

  Ok(Statement { path, value })
}

/// Split a path string on `.` into [`Segment`]s. Integer segments
/// are parsed eagerly so callers can dispatch on type rather than
/// re-parsing the value at every grammar arm.
///
/// Segment character class: lowercase ASCII letters, digits,
/// underscore, `+`, and `-`. `+` / `-` carry semantic meaning in
/// stat-modifier aspect names (`corpus+`, `corpus-`, `corpus++`) and
/// don't collide with the `.` segment separator or the `: ` value
/// separator. Anything else (`:`, whitespace, uppercase, `*`, `/`,
/// quotes, brackets) is still rejected so authoring mistakes like
/// `style.default: ` (the `:` collapses into the last segment after
/// the trailing space is trimmed) fail loudly instead of
/// round-tripping through the parser.
fn parse_path(s: &str) -> Result<Vec<Segment>, String> {
  if s.is_empty() {
    return Err("empty path".to_string());
  }
  let mut out = Vec::new();
  for raw in s.split('.') {
    if raw.is_empty() {
      return Err(format!("path {s:?}: empty segment (double-dot or leading/trailing dot)"));
    }
    for c in raw.chars() {
      if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '+' || c == '-') {
        return Err(format!(
          "path {s:?}: segment {raw:?} contains invalid character {c:?} \
           (segments must be ascii lowercase letters, digits, underscore, '+', or '-')"
        ));
      }
    }
    if let Ok(n) = raw.parse::<u32>() {
      out.push(Segment::Index(n));
    } else {
      out.push(Segment::Word(raw.to_string()));
    }
  }
  Ok(out)
}

fn parse_statement_value(s: &str) -> StatementValue {
  if let Ok(n) = s.parse::<i64>() {
    StatementValue::Int(n)
  } else {
    StatementValue::Str(s.to_string())
  }
}

// ---------- Reserved-word check ------------------------------------------

/// Closed list of identifier tokens that appear as path-grammar
/// segments. Aspect names (and other content-namespace identifiers
/// that flow into path segments) must not collide with these —
/// otherwise paths like `input.hex.aspect.<name>.min` become
/// ambiguous between "aspect named `min`" vs "predicate operator
/// `min`."
///
/// Kept as a hard-coded constant rather than derived from a JSON
/// registry to avoid bootstrap ordering: the recipe loader runs
/// after `aspects.json` parses, and we want this check to fire
/// during aspect parsing (a reserved-name aspect should fail at the
/// aspect-registry build, not deep inside the recipe registry).
pub const RESERVED_PATH_TOKENS: &[&str] = &[
  // Top-level buckets (legacy — bucketed `input.X` / `output.X` form)
  "input",
  "output",
  "duration",
  "style",
  // Anchors / refs
  "root",
  "slot",
  // Namespace anchors (tape form)
  "sys",
  "var",
  // Card resolvers (after an anchor)
  "owner",
  "parent",
  // Match selectors
  "def_id",
  "aspect",
  // Predicate operators
  "min",
  "eq",
  "ne",
  "gt",
  "ge",
  "lt",
  "le",
  // Output effect buckets (legacy bucketed form)
  "create",
  "destroy",
  "modify",
  // Assignment / arithmetic ops (both bucketed and tape forms)
  "add",
  "sub",
  "set",
  // Tape-form generator op
  "random",
  // Locations
  "inventory",
  // Duration tier markers (legacy)
  "default",
  "when",
];

/// True iff `name` collides with a reserved path token. Callers
/// (typically the aspect-registry builder) reject identifiers that
/// would shadow grammar tokens at registry build time.
pub fn is_reserved_aspect_name(name: &str) -> bool {
  RESERVED_PATH_TOKENS.iter().any(|t| *t == name)
}

// ---------- Tests ---------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

  fn word(s: &str) -> Segment {
    Segment::Word(s.to_string())
  }
  fn idx(n: u32) -> Segment {
    Segment::Index(n)
  }

  // ---- statement parser ------------------------------------------------

  #[test]
  fn parses_verb_only_statement() {
    let s = parse_statement("output.destroy.slot.0").unwrap();
    assert_eq!(
      s.path,
      vec![word("output"), word("destroy"), word("slot"), idx(0)]
    );
    assert_eq!(s.value, None);
  }

  #[test]
  fn parses_path_plus_integer_value() {
    let s = parse_statement("input.hex.aspect.wood.min: 1").unwrap();
    assert_eq!(
      s.path,
      vec![
        word("input"),
        word("hex"),
        word("aspect"),
        word("wood"),
        word("min")
      ]
    );
    assert_eq!(s.value, Some(StatementValue::Int(1)));
  }

  #[test]
  fn parses_path_plus_string_value() {
    let s = parse_statement("style.default: rtl").unwrap();
    assert_eq!(s.path, vec![word("style"), word("default")]);
    assert_eq!(s.value, Some(StatementValue::Str("rtl".to_string())));
  }

  #[test]
  fn parses_card_key_value_with_hyphen() {
    let s = parse_statement("output.create.actor.inventory: corpus-").unwrap();
    assert_eq!(
      s.value,
      Some(StatementValue::Str("corpus-".to_string()))
    );
  }

  #[test]
  fn parses_slot_index_after_word() {
    let s = parse_statement("input.slot.0.def_id: corpus").unwrap();
    assert_eq!(
      s.path,
      vec![word("input"), word("slot"), idx(0), word("def_id")]
    );
  }

  #[test]
  fn parses_owner_chain() {
    let s = parse_statement("output.create.slot.0.owner.inventory: log").unwrap();
    assert_eq!(
      s.path,
      vec![
        word("output"),
        word("create"),
        word("slot"),
        idx(0),
        word("owner"),
        word("inventory")
      ]
    );
  }

  #[test]
  fn rejects_empty_statement() {
    assert!(parse_statement("").is_err());
    assert!(parse_statement("   ").is_err());
  }

  #[test]
  fn rejects_empty_value() {
    assert!(parse_statement("style.default: ").is_err());
  }

  #[test]
  fn rejects_empty_segment() {
    assert!(parse_statement("input..wood").is_err());
    assert!(parse_statement(".input").is_err());
    assert!(parse_statement("input.").is_err());
  }

  #[test]
  fn rejects_whitespace_inside_segment() {
    assert!(parse_statement("input. hex").is_err());
    assert!(parse_statement("input.he x.def_id: corpus").is_err());
  }

  #[test]
  fn rejects_integer_first_segment() {
    assert!(parse_statement("0.aspect.wood").is_err());
  }

  #[test]
  fn parses_aspect_name_with_plus_suffix() {
    let s = parse_statement("input.slot.0.aspect.corpus+.min: 1").unwrap();
    assert_eq!(
      s.path,
      vec![
        word("input"),
        word("slot"),
        idx(0),
        word("aspect"),
        word("corpus+"),
        word("min"),
      ]
    );
    assert_eq!(s.value, Some(StatementValue::Int(1)));
  }

  #[test]
  fn parses_aspect_name_with_minus_suffix() {
    let s = parse_statement("input.slot.0.aspect.corpus-.min: 1").unwrap();
    assert!(s.path.iter().any(|seg| seg.as_word() == Some("corpus-")));
  }

  #[test]
  fn parses_doubled_plus_suffix() {
    let s = parse_statement("input.slot.0.aspect.corpus++.min: 1").unwrap();
    assert!(s.path.iter().any(|seg| seg.as_word() == Some("corpus++")));
  }

  #[test]
  fn still_rejects_colon_in_segment() {
    assert!(parse_statement("style.default:").is_err());
  }

  #[test]
  fn still_rejects_uppercase_in_segment() {
    assert!(parse_statement("input.Slot.0.def_id: corpus").is_err());
  }

  #[test]
  fn parses_duration_with_when_predicate() {
    let s = parse_statement("duration.when.aspect.fleeting.min.4: 20").unwrap();
    assert_eq!(
      s.path,
      vec![
        word("duration"),
        word("when"),
        word("aspect"),
        word("fleeting"),
        word("min"),
        idx(4)
      ]
    );
    assert_eq!(s.value, Some(StatementValue::Int(20)));
  }

  #[test]
  fn parses_negative_integer_value() {
    let s = parse_statement("debug.delta: -3").unwrap();
    assert_eq!(s.value, Some(StatementValue::Int(-3)));
  }

  #[test]
  fn bucket_returns_first_word() {
    let s = parse_statement("output.destroy.slot.0").unwrap();
    assert_eq!(s.bucket(), Some("output"));
  }

  // ---- reserved-word check --------------------------------------------

  #[test]
  fn flags_collision_with_grammar_token() {
    assert!(is_reserved_aspect_name("min"));
    assert!(is_reserved_aspect_name("sub"));
    assert!(is_reserved_aspect_name("default"));
    assert!(is_reserved_aspect_name("inventory"));
  }

  #[test]
  fn accepts_safe_aspect_names() {
    assert!(!is_reserved_aspect_name("wood"));
    assert!(!is_reserved_aspect_name("stone"));
    assert!(!is_reserved_aspect_name("flora"));
    assert!(!is_reserved_aspect_name("corpus"));
    assert!(!is_reserved_aspect_name("fleeting"));
  }
}
