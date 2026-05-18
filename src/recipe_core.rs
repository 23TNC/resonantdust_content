//! Recipe registry — flat namespace backed by the tape-form parser.
//!
//! Recipes are loaded from JSON files under `content/recipes/data/`,
//! parsed via [`crate::recipe_tape::parse_file`], cross-referenced with
//! the stable id map in `content/recipes/id.json`, and exposed as a
//! registry with `u16`-keyed lookup.
//!
//! Re-exports the tape-form types ([`Recipe`], [`Seg`], [`Iterator`],
//! [`Stmt`], [`Direction`], [`AnchorSet`]) so downstream consumers can
//! import everything from one place.
//!
//! # JSON shape (per data file)
//!
//! ```jsonc
//! {
//!   "<recipe-id>": {
//!     "input":  ["<path>.<op>[: value]", ...],
//!     "output": ["<path>.<op>[: value]", ...]
//!   },
//!   ...
//! }
//! ```
//!
//! See [`crate::recipe_tape`] for the path-first statement grammar,
//! iterator extraction, and anchor-set classification.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::embedded_data::RECIPES_FILES;

pub use crate::recipe_tape::{
  AnchorSet, Direction, Iterator, Recipe, Seg, Stmt,
};

const RECIPE_IDS_JSON: &str = include_str!("../recipes/id.json");

struct RecipeRegistry {
  by_id: BTreeMap<u16, Recipe>,
  id_by_name: BTreeMap<String, u16>,
}

static RECIPES: OnceLock<Result<RecipeRegistry, String>> = OnceLock::new();

fn recipes_registry() -> Result<&'static RecipeRegistry, String> {
  RECIPES.get_or_init(build_recipes).as_ref().map_err(|e| e.clone())
}

/// Look up a recipe by its stable `u16` id from `recipes/id.json`.
/// Returns `Ok(None)` if no recipe is registered under that id.
pub fn recipe(id: u16) -> Result<Option<&'static Recipe>, String> {
  Ok(recipes_registry()?.by_id.get(&id))
}

/// Look up a recipe by its source-key (e.g. `"cut_tree"`).
pub fn find_recipe(key: &str) -> Result<Option<&'static Recipe>, String> {
  let r = recipes_registry()?;
  let Some(&id) = r.id_by_name.get(key) else {
    return Ok(None);
  };
  Ok(r.by_id.get(&id))
}

/// Look up a recipe's stable `u16` id by source-key. Returns `Ok(None)`
/// if no recipe with that key is registered. Cheaper than [`find_recipe`]
/// when only the id is needed (e.g., looking up a card def's
/// lifecycle/magnetic recipe reference).
pub fn find_recipe_id(key: &str) -> Result<Option<u16>, String> {
  let r = recipes_registry()?;
  Ok(r.id_by_name.get(key).copied())
}

/// All recipes in priority-tiered order, highest priority first.
/// Used by the client matcher's prefilter — the matcher walks
/// recipes in this order and stops at the first tier with matches.
///
/// Priority is determined by [`AnchorSet::priority_key`] —
/// anchor-count first, then anchor-priority (`hex > root > up > down`).
pub fn recipes_by_priority() -> Result<Vec<&'static Recipe>, String> {
  let r = recipes_registry()?;
  let mut all: Vec<&Recipe> = r.by_id.values().collect();
  all.sort_by_key(|r| std::cmp::Reverse(r.anchors.priority_key()));
  Ok(all)
}

fn build_recipes() -> Result<RecipeRegistry, String> {
  // Read flat id map `{ "<recipe-id>": <u16>, ... }`.
  let ids_value: Value = serde_json::from_str(RECIPE_IDS_JSON)
    .map_err(|e| format!("recipes/id.json: parse failed: {e}"))?;
  let ids_obj = ids_value.as_object().ok_or_else(|| {
    "recipes/id.json: top-level must be a flat object {recipe-id: u16}".to_string()
  })?;
  let mut id_by_name: BTreeMap<String, u16> = BTreeMap::new();
  for (key, val) in ids_obj {
    let n = val.as_u64().ok_or_else(|| {
      format!("recipes/id.json: id for {:?} is not an integer", key)
    })?;
    if n == 0 || n > u16::MAX as u64 {
      return Err(format!(
        "recipes/id.json: id {} for {:?} out of range (1..={})",
        n,
        key,
        u16::MAX,
      ));
    }
    id_by_name.insert(key.clone(), n as u16);
  }

  // Parse every embedded recipe file. Each parsed recipe's source-key
  // is matched against `id_by_name` to assign its stable u16 id.
  // Recipes declared in a data file but missing from id.json error —
  // `gen-ids.py` is the source of truth and should be run first.
  let mut by_id: BTreeMap<u16, Recipe> = BTreeMap::new();
  for (filename, content) in RECIPES_FILES {
    let recipes = crate::recipe_tape::parse_file(filename, content)?;
    for recipe in recipes {
      let id = id_by_name.get(&recipe.id).copied().ok_or_else(|| {
        format!(
          "{filename}: recipe {:?} missing from recipes/id.json (run gen-ids.py)",
          recipe.id
        )
      })?;
      if by_id.contains_key(&id) {
        return Err(format!(
          "{filename}: duplicate id {} (recipe {:?})",
          id, recipe.id
        ));
      }
      by_id.insert(id, recipe);
    }
  }

  Ok(RecipeRegistry { by_id, id_by_name })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn registry_builds_from_embedded_data() {
    let r = recipes_registry().expect("registry");
    // Migration baseline: 12 recipes across 01.json + 02.json.
    assert!(
      r.by_id.len() >= 12,
      "expected at least 12 recipes; got {}",
      r.by_id.len()
    );
  }

  #[test]
  fn find_recipe_resolves_known_key() {
    let r = find_recipe("cut_tree")
      .expect("find_recipe")
      .expect("cut_tree present");
    assert_eq!(r.id, "cut_tree");
    assert!(r.anchors.hex, "cut_tree must require hex");
  }

  #[test]
  fn find_recipe_id_resolves_known_key() {
    let id = find_recipe_id("cut_tree")
      .expect("find_recipe_id")
      .expect("cut_tree present");
    // The id must round-trip through `recipe(id)`.
    let r = recipe(id).expect("recipe(id)").expect("present");
    assert_eq!(r.id, "cut_tree");
  }

  #[test]
  fn find_recipe_misses_unknown_key() {
    let opt = find_recipe("not_a_real_recipe").expect("find_recipe");
    assert!(opt.is_none());
  }

  #[test]
  fn recipes_by_priority_orders_descending() {
    let all = recipes_by_priority().expect("priority list");
    for window in all.windows(2) {
      assert!(
        window[0].anchors.priority_key() >= window[1].anchors.priority_key(),
        "out of order: {} ({:?}) vs {} ({:?})",
        window[0].id,
        window[0].anchors,
        window[1].id,
        window[1].anchors,
      );
    }
  }
}
