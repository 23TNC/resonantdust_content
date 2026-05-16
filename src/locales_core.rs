//! Locale registry built from JSON catalogs in `content/locales/`.
//!
//! Each locale file is `locales/<domain>/<lang>.json` where `<domain>`
//! mirrors a source-data tree name (`cards`, `recipes`, …) and
//! `<lang>` is a short language code (`en`, future `fr`, etc.). The
//! file's JSON shape mirrors the corresponding `id.json` structure
//! exactly — `locales/cards/en.json` is path-keyed by
//! `<card_type>/<key>`, `locales/recipes/en.json` by
//! `<recipe_type>/<category>/<key>`, etc.
//!
//! ```jsonc
//! // locales/cards/en.json
//! {
//!   "requisite": {
//!     "log": {
//!       "label": "Log",
//!       "description": { "simple": "Just a wooden log." }
//!     }
//!   }
//! }
//! ```
//!
//! Each leaf (object containing a `label` field) is a [`LocaleEntry`].
//! The loader recursively descends the tree; any object with a `label`
//! string is treated as a leaf and the JSON-path traversed so far is
//! its lookup path. Other fields on the leaf object (today: `description`,
//! recipe-side `success`) are stashed under
//! [`LocaleEntry::descriptions`] / etc.
//!
//! # Lookups
//!
//! [`label`] / [`description`] take `(domain, lang, path)` and apply a
//! two-step fallback:
//!
//! 1. Look up `(domain, lang, path)` directly. Hit → return.
//! 2. Look up `(domain, "en", path)`. Hit → return.
//! 3. Miss → `None`. Caller renders the bare key as a dev-side
//!    fallback (and ideally `console.warn`s during development).
//!
//! Strict-in-CI is the recipe author's job: `bin/content check` is the
//! place to fail on missing entries, not the lookup hot path.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::embedded_data::LOCALES_FILES;

/// One translation entry. Today carries a `label` (the user-facing
/// short name) plus an open-ended `descriptions` map keyed by variant
/// name. Cards declare `description.simple` / `description.detailed`;
/// recipes declare `success.simple` (event-log line on success) under
/// the same struct. Both use the same map — variant names are the
/// "namespace" within the entry.
#[derive(Debug, Clone)]
pub struct LocaleEntry {
  /// Required. User-facing short name. Empty string is a valid value
  /// (e.g. for entries that exist only to carry descriptions).
  pub label: String,
  /// Variant → text. Variant keys are author-defined; today's
  /// vocabulary is `description.simple` / `description.detailed` for
  /// cards and `success.simple` for recipes. The loader flattens the
  /// nested JSON object into dotted keys: `description.simple` and
  /// `success.simple` both land here, so a single lookup function
  /// covers both.
  pub variants: BTreeMap<String, String>,
}

/// Maximum recursive descent depth while walking a locale file. Above
/// this is treated as malformed (unbounded recursion is impossible
/// since the JSON's object tree is finite, but a sanity cap surfaces
/// pathological files early). Real depth today is 3-4 levels.
const MAX_WALK_DEPTH: usize = 32;

struct LocaleRegistry {
  /// `(domain, lang, dotted_path)` → entry.
  entries: BTreeMap<(String, String, String), LocaleEntry>,
  /// `domain` → sorted set of language codes declared.
  languages_by_domain: BTreeMap<String, Vec<String>>,
}

static LOCALES: OnceLock<Result<LocaleRegistry, String>> = OnceLock::new();

fn locales_registry() -> Result<&'static LocaleRegistry, String> {
  LOCALES.get_or_init(build_locales).as_ref().map_err(|e| e.clone())
}

/// Look up the label for `path` in `(domain, lang)`, falling back to
/// `(domain, "en")`. Returns `Ok(None)` when neither registers an
/// entry. `Err` on registry-build failure.
///
/// `path` is dotted — `"requisite.default.log"` /
/// `"stack.up.cut_tree"`. Mirrors how the source data files identify
/// the same entity (matches `id.json` keys).
pub fn label(domain: &str, lang: &str, path: &str) -> Result<Option<&'static str>, String> {
  let registry = locales_registry()?;
  if let Some(entry) = lookup_entry(registry, domain, lang, path) {
    return Ok(Some(entry.label.as_str()));
  }
  if lang != "en" {
    if let Some(entry) = lookup_entry(registry, domain, "en", path) {
      return Ok(Some(entry.label.as_str()));
    }
  }
  Ok(None)
}

/// Look up a variant text for `path` in `(domain, lang)`. `variant` is
/// dotted (`"description.simple"`, `"description.detailed"`,
/// `"success.simple"`, …). Same fallback chain as [`label`]. Returns
/// `Ok(None)` when the entry exists but doesn't declare this variant,
/// or when neither language has the entry.
pub fn variant(
  domain: &str,
  lang: &str,
  path: &str,
  variant: &str,
) -> Result<Option<&'static str>, String> {
  let registry = locales_registry()?;
  if let Some(entry) = lookup_entry(registry, domain, lang, path) {
    if let Some(text) = entry.variants.get(variant) {
      return Ok(Some(text.as_str()));
    }
  }
  if lang != "en" {
    if let Some(entry) = lookup_entry(registry, domain, "en", path) {
      if let Some(text) = entry.variants.get(variant) {
        return Ok(Some(text.as_str()));
      }
    }
  }
  Ok(None)
}

/// Fetch the raw [`LocaleEntry`] (current-language only, no fallback).
/// Useful for UIs that want to inspect every available variant rather
/// than ask for one at a time.
pub fn entry(
  domain: &str,
  lang: &str,
  path: &str,
) -> Result<Option<&'static LocaleEntry>, String> {
  let registry = locales_registry()?;
  Ok(lookup_entry(registry, domain, lang, path))
}

/// Language codes declared for a domain, sorted. Useful for UI
/// language selectors — `languages("cards")` returns every language
/// with at least one entry in `locales/cards/`. Empty when the domain
/// has no locale files yet.
pub fn languages(domain: &str) -> Result<&'static [String], String> {
  let registry = locales_registry()?;
  Ok(
    registry
      .languages_by_domain
      .get(domain)
      .map(|v| v.as_slice())
      .unwrap_or(&[]),
  )
}

fn lookup_entry<'a>(
  registry: &'a LocaleRegistry,
  domain: &str,
  lang: &str,
  path: &str,
) -> Option<&'a LocaleEntry> {
  registry
    .entries
    .get(&(domain.to_string(), lang.to_string(), path.to_string()))
}

fn build_locales() -> Result<LocaleRegistry, String> {
  let mut entries: BTreeMap<(String, String, String), LocaleEntry> = BTreeMap::new();
  let mut languages_by_domain: BTreeMap<String, std::collections::BTreeSet<String>> =
    BTreeMap::new();

  for (rel_path, content) in LOCALES_FILES {
    // `rel_path` is like `"locales/cards/en.json"` — derived by
    // `build.rs::emit_list` joining `rel_dir` and the per-file relative
    // path. Split into `(domain, lang)`.
    let path_under_locales = rel_path
      .strip_prefix("locales/")
      .ok_or_else(|| format!("{}: not under 'locales/' (build.rs invariant)", rel_path))?;
    let parts: Vec<&str> = path_under_locales.split('/').collect();
    if parts.len() != 2 {
      return Err(format!(
        "{}: expected exactly `<domain>/<lang>.json` (got {} segments)",
        rel_path,
        parts.len()
      ));
    }
    let domain = parts[0].to_string();
    let filename = parts[1];
    let lang = filename
      .strip_suffix(".json")
      .ok_or_else(|| format!("{}: filename must end in .json", rel_path))?
      .to_string();

    let parsed: Value = serde_json::from_str(content)
      .map_err(|e| format!("{}: parse failed: {}", rel_path, e))?;
    let root = parsed.as_object().ok_or_else(|| {
      format!("{}: top-level must be an object keyed by path segments", rel_path)
    })?;

    let mut path_stack: Vec<String> = Vec::new();
    walk(rel_path, root, &mut path_stack, &mut |path, leaf| {
      let key = (domain.clone(), lang.clone(), path.join("."));
      if entries.contains_key(&key) {
        return Err(format!(
          "{}: duplicate locale entry for path {:?} in language {:?}",
          rel_path,
          path.join("."),
          lang
        ));
      }
      entries.insert(key, leaf);
      Ok(())
    })?;

    languages_by_domain
      .entry(domain)
      .or_default()
      .insert(lang);
  }

  let languages_by_domain = languages_by_domain
    .into_iter()
    .map(|(domain, langs)| (domain, langs.into_iter().collect::<Vec<_>>()))
    .collect();

  Ok(LocaleRegistry {
    entries,
    languages_by_domain,
  })
}

/// Recursively descend `obj`. Any nested object that has a `label`
/// string field is a leaf — collected via `emit`. Other nested objects
/// extend the path stack and recurse. Non-string `label` values or
/// non-object intermediates fail loudly so a typo doesn't silently
/// drop a leaf.
fn walk(
  rel_path: &str,
  obj: &serde_json::Map<String, Value>,
  path_stack: &mut Vec<String>,
  emit: &mut impl FnMut(&[String], LocaleEntry) -> Result<(), String>,
) -> Result<(), String> {
  if path_stack.len() > MAX_WALK_DEPTH {
    return Err(format!(
      "{}: locale tree exceeds max depth {} at path {:?}",
      rel_path, MAX_WALK_DEPTH, path_stack
    ));
  }
  // Leaf detection: an object with a `label: <string>` field is an
  // entry; everything else inside it is recorded as a variant.
  if let Some(label_val) = obj.get("label") {
    let label = label_val.as_str().ok_or_else(|| {
      format!(
        "{}: entry at {:?}: 'label' must be a string",
        rel_path, path_stack
      )
    })?;
    let mut variants: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in obj {
      if k == "label" {
        continue;
      }
      flatten_variant_tree(rel_path, path_stack, k, v, &mut variants)?;
    }
    emit(
      path_stack,
      LocaleEntry {
        label: label.to_string(),
        variants,
      },
    )?;
    return Ok(());
  }
  // Non-leaf: every value must be a child object, and its key extends
  // the path stack.
  for (k, v) in obj {
    let child = v.as_object().ok_or_else(|| {
      format!(
        "{}: at path {:?}: child {:?} must be either an object with a 'label' field (leaf) or an object of further sub-paths",
        rel_path, path_stack, k
      )
    })?;
    path_stack.push(k.clone());
    walk(rel_path, child, path_stack, emit)?;
    path_stack.pop();
  }
  Ok(())
}

/// Flatten a non-`label` field into dotted variant keys. A bare string
/// at the top is recorded under the field's own name (e.g. an entry
/// with `"tooltip": "..."`); an object recurses with `<field>.<sub>`
/// keys (e.g. `description.simple`, `description.detailed`).
fn flatten_variant_tree(
  rel_path: &str,
  path_stack: &[String],
  key: &str,
  value: &Value,
  out: &mut BTreeMap<String, String>,
) -> Result<(), String> {
  match value {
    Value::String(s) => {
      out.insert(key.to_string(), s.clone());
    }
    Value::Object(obj) => {
      for (sub_key, sub_val) in obj {
        let composite = format!("{}.{}", key, sub_key);
        flatten_variant_tree(rel_path, path_stack, &composite, sub_val, out)?;
      }
    }
    other => {
      return Err(format!(
        "{}: entry at {:?}: variant {:?} must be a string or object of strings; got {:?}",
        rel_path, path_stack, key, other
      ));
    }
  }
  Ok(())
}
