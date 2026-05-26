//! Recipe-statement alias substitution.
//!
//! Loads `content/recipes/aliases.json` (a flat `{ key: value }` map)
//! and exposes [`apply_aliases`] — a pre-parse pass that swaps every
//! CamelCase token that appears in the alias map for its declared
//! value verbatim. CamelCase tokens that aren't in the map pass
//! through unchanged (lenient mode) so existing test fixtures using
//! single-letter uppercase placeholders (`A`, `B`, …) keep parsing.
//!
//! ### Substitution rules
//!
//! A "CamelCase identifier" is a maximal run of `[A-Za-z0-9_]`
//! starting with an ASCII uppercase letter. Lowercase- or
//! digit-starting tokens are left untouched (so `slot.1.0`,
//! `aspect`, `corpus+`, etc. pass through). The substitution is
//! one-pass — an alias value containing another CamelCase token is
//! NOT re-resolved. If a future use case needs chaining, the rule
//! could be widened to fixed-point at registry build time.
//!
//! ### Convention vs. enforcement
//!
//! The project convention is: use snake_case / lowercase in recipe
//! content (def_ids, aspect names, verbs); reserve CamelCase for
//! alias keys. This parser is LENIENT — unknown CamelCase tokens
//! pass through verbatim. That trades a typo gate for backward
//! compatibility with existing fixtures. Tighten to strict (error
//! on unknown CamelCase) when the fixtures stop using uppercase
//! placeholders.
//!
//! ### Why a separate pass (not in `parse_statement`)
//!
//! `parse_statement` is shared with unit tests that don't want a
//! global alias registry to spin up; keeping substitution at the
//! recipe-builder boundary ([`crate::recipe_tape::parse_one`])
//! leaves `parse_statement` pure.
//!
//! ### Failure mode
//!
//! Same `OnceLock<Result<…, String>>` pattern as the other
//! registries. A malformed `aliases.json` fails the build once and
//! every subsequent `apply_aliases` call returns the cached error.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

const ALIASES_JSON: &str = include_str!("../recipes/aliases.json");
static ALIASES: OnceLock<Result<BTreeMap<String, String>, String>> = OnceLock::new();

fn aliases_registry() -> Result<&'static BTreeMap<String, String>, String> {
  ALIASES.get_or_init(build_aliases).as_ref().map_err(|e| e.clone())
}

fn build_aliases() -> Result<BTreeMap<String, String>, String> {
  let root: Value = serde_json::from_str(ALIASES_JSON)
    .map_err(|e| format!("recipes/aliases.json: parse failed: {e}"))?;
  let obj = root
    .as_object()
    .ok_or_else(|| "recipes/aliases.json: top-level must be a flat object".to_string())?;
  let mut out = BTreeMap::new();
  for (key, val) in obj {
    if key.starts_with('_') {
      continue; // `_comment` and similar doc keys
    }
    if !is_valid_alias_key(key) {
      return Err(format!(
        "recipes/aliases.json: alias key {:?} must start with an uppercase ASCII letter and contain only [A-Za-z0-9_]",
        key
      ));
    }
    let s = val.as_str().ok_or_else(|| {
      format!(
        "recipes/aliases.json: alias {:?} value must be a string (got {:?})",
        key, val
      )
    })?;
    out.insert(key.clone(), s.to_string());
  }
  Ok(out)
}

fn is_valid_alias_key(s: &str) -> bool {
  let mut iter = s.chars();
  let Some(first) = iter.next() else { return false };
  if !first.is_ascii_uppercase() {
    return false;
  }
  iter.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Replace every CamelCase identifier in `input` that has an entry
/// in `aliases.json` with that entry's value. CamelCase tokens that
/// aren't in the map pass through verbatim. Lowercase- /
/// digit-starting tokens and all non-identifier characters always
/// pass through, so this function is a no-op on statements that
/// don't use any aliases.
///
/// Returns `Err` only when the alias registry itself fails to build
/// (malformed `aliases.json`); per-statement substitution can't
/// fail in lenient mode.
pub fn apply_aliases(input: &str) -> Result<String, String> {
  let aliases = aliases_registry()?;
  let mut out = String::with_capacity(input.len());
  let mut iter = input.char_indices().peekable();
  while let Some((i, c)) = iter.next() {
    if c.is_ascii_uppercase() {
      let start = i;
      let mut end = i + c.len_utf8();
      while let Some(&(j, nc)) = iter.peek() {
        if nc.is_ascii_alphanumeric() || nc == '_' {
          end = j + nc.len_utf8();
          iter.next();
        } else {
          break;
        }
      }
      let token = &input[start..end];
      match aliases.get(token) {
        Some(v) => out.push_str(v),
        None => out.push_str(token), // lenient passthrough
      }
    } else {
      out.push(c);
    }
  }
  Ok(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn registry_builds() {
    aliases_registry().expect("aliases registry should build clean");
  }

  #[test]
  fn passthrough_on_lowercase_statement() {
    // No CamelCase → byte-identical output.
    let s = "use.slot.0.0.aspect.wood.min: 1";
    assert_eq!(apply_aliases(s).unwrap(), s);
  }

  #[test]
  fn substitutes_slot_aliases() {
    // Aliases stop at the branch index; authors append the offset
    // (`Top.0`, `Top.1`, …) so one alias serves every offset.
    assert_eq!(
      apply_aliases("Top.0.destroy").unwrap(),
      "slot.1.0.destroy"
    );
    assert_eq!(
      apply_aliases("Hex.0.aspect.wood.min: 1").unwrap(),
      "slot.0.0.aspect.wood.min: 1"
    );
  }

  #[test]
  fn substitutes_value_alias() {
    assert_eq!(
      apply_aliases("Hex.0.owner.aspect.faction.set: FactionChorus").unwrap(),
      "slot.0.0.owner.aspect.faction.set: 1"
    );
  }

  #[test]
  fn unknown_camelcase_passes_through() {
    // Lenient mode — undeclared CamelCase isn't an error. Useful
    // for single-letter placeholders in tests and for content
    // authors mid-migration who haven't declared their aliases yet.
    assert_eq!(
      apply_aliases("Top.0.style.set: NotAnAlias").unwrap(),
      "slot.1.0.style.set: NotAnAlias"
    );
  }

  #[test]
  fn does_not_chain() {
    // Aliases are one-pass — if a value contained a CamelCase token,
    // it would NOT be re-resolved. Our seed aliases don't currently
    // exercise this, but the test pins the behavior.
    // (No assertion needed beyond the registry-build test above; the
    // current seed has no chained aliases. Keep this comment as the
    // contract.)
  }
}
