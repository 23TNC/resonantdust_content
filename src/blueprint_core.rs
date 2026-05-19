//! Blueprint registry.
//!
//! A blueprint is a named "recipe to build a card" — at this point each
//! entry just carries a target `card_id` (the card the blueprint
//! produces). The schema will grow later to include build cost,
//! prerequisites, output stats, etc.; this initial pass establishes the
//! `(key → stable id → resolved card)` plumbing so the wrench panel
//! can render the catalog and `proposeAction`-style flows can address
//! a blueprint by its u16 wire id.
//!
//! # Loading
//!
//! Source data lives in `<repo>/content/blueprints/`:
//!
//! - `blueprints/id.json` — stable id map produced by `gen-ids.py`.
//!   Format: `{ "<blueprint_key>": <int>, ... }` — flat namespace, same
//!   shape as `recipes/id.json`.
//! - `blueprints/data/**/*.json` — each file is a top-level
//!   `{ "<blueprint_key>": { "blueprint": "<bp_card_key>", "card":
//!   "<out_card_key>", ... } }` object. `blueprint` is the card the UI
//!   draws when the blueprint is discovered (wrench panel /
//!   character-create preview); `card` is the output card that gets
//!   produced when the blueprint is eventually built. Additional body
//!   fields are tolerated and ignored for now.
//!
//! Both keys resolve to `packed_definition` at registry build time via
//! [`crate::definition_core::find_packed_by_key`] — an unknown key
//! becomes a stored registry-build error rather than a runtime spawn
//! failure.
//!
//! # Failure mode
//!
//! Same `OnceLock<Result<Registry, String>>` pattern as
//! `definition_core` / `recipe_core` / `starter_pack_core`: a malformed
//! file fails the build once and every subsequent lookup returns the
//! cached error.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::definition_core::find_packed_by_key;
use crate::embedded_data::BLUEPRINTS_FILES;

pub type BlueprintId = u16;

/// Sentinel id meaning "no blueprint". Blueprint IDs are 1-indexed in
/// `blueprints/id.json`.
pub const BLUEPRINT_NONE: BlueprintId = 0;

const BLUEPRINT_IDS_JSON: &str = include_str!("../blueprints/id.json");

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blueprint {
  /// Stable id from `blueprints/id.json`. 1-indexed; 0 is reserved as
  /// `BLUEPRINT_NONE`.
  pub id: BlueprintId,
  /// Blueprint key from the source JSON, e.g. `"nd_furnace"`.
  pub key: String,
  /// Blueprint *card* key — the card visual the UI draws when this
  /// blueprint is discovered (wrench panel, character-create
  /// preview). Sourced from the `"blueprint"` field. Validated as a
  /// known card key at registry build time.
  pub blueprint_key: String,
  /// `packed_definition` for `blueprint_key`, resolved via
  /// [`find_packed_by_key`].
  pub blueprint_packed_definition: u16,
  /// Output card key — the card produced when the blueprint is
  /// eventually built in-world. Sourced from the `"card"` field. May
  /// be folded into a different schema later; for now it carries
  /// alongside `blueprint_key` so consumers can resolve "what does
  /// this blueprint make" without a second lookup.
  pub card_key: String,
  /// `packed_definition` for `card_key`.
  pub card_packed_definition: u16,
}

struct BlueprintRegistry {
  by_id: BTreeMap<BlueprintId, Blueprint>,
  by_key: BTreeMap<String, BlueprintId>,
  /// Stable-id-sorted list for deterministic iteration (UI rendering,
  /// catalog enumeration).
  ordered: Vec<BlueprintId>,
}

static BLUEPRINTS: OnceLock<Result<BlueprintRegistry, String>> = OnceLock::new();

fn registry() -> Result<&'static BlueprintRegistry, String> {
  BLUEPRINTS.get_or_init(build).as_ref().map_err(|e| e.clone())
}

/// Look up a blueprint by its stable id. `Ok(None)` for
/// `BLUEPRINT_NONE` or for ids not present in the registry; `Err` on
/// registry-build failure.
pub fn blueprint(id: BlueprintId) -> Result<Option<&'static Blueprint>, String> {
  if id == BLUEPRINT_NONE {
    return Ok(None);
  }
  Ok(registry()?.by_id.get(&id))
}

/// Look up a blueprint by its source-key (e.g. `"nd_furnace"`).
/// `Ok(None)` if no blueprint with that key is registered; `Err` on
/// registry-build failure.
pub fn find_blueprint(key: &str) -> Result<Option<&'static Blueprint>, String> {
  let registry = registry()?;
  let Some(&id) = registry.by_key.get(key) else {
    return Ok(None);
  };
  Ok(registry.by_id.get(&id))
}

/// Every registered blueprint in stable-id order. Empty vec when no
/// blueprints are declared. `Err` on registry-build failure.
pub fn blueprints_all() -> Result<Vec<&'static Blueprint>, String> {
  let registry = registry()?;
  Ok(registry.ordered.iter().filter_map(|id| registry.by_id.get(id)).collect())
}

fn build() -> Result<BlueprintRegistry, String> {
  // 1. Load stable id map: { "<blueprint_key>": <int> }.
  let id_root: Value = serde_json::from_str(BLUEPRINT_IDS_JSON)
    .map_err(|e| format!("blueprints/id.json: parse failed: {}", e))?;
  let id_obj = id_root.as_object().ok_or_else(|| {
    "blueprints/id.json: top-level not an object".to_string()
  })?;
  let mut stable_ids: BTreeMap<String, BlueprintId> = BTreeMap::new();
  for (key, id_value) in id_obj {
    let n = id_value.as_u64().ok_or_else(|| {
      format!("blueprints/id.json: id for {:?} not an integer", key)
    })?;
    if n == 0 || n > BlueprintId::MAX as u64 {
      return Err(format!(
        "blueprints/id.json: id {} for {:?} out of range (1..={})",
        n,
        key,
        BlueprintId::MAX
      ));
    }
    stable_ids.insert(key.clone(), n as BlueprintId);
  }

  // 2. Parse each data file.
  let mut by_id: BTreeMap<BlueprintId, Blueprint> = BTreeMap::new();
  let mut by_key: BTreeMap<String, BlueprintId> = BTreeMap::new();

  for (filename, content) in BLUEPRINTS_FILES {
    let parsed: Value = serde_json::from_str(content)
      .map_err(|e| format!("{}: parse failed: {}", filename, e))?;
    let entry_obj = parsed.as_object().ok_or_else(|| {
      format!("{}: top-level must be an object keyed by blueprint key", filename)
    })?;
    for (key, body_val) in entry_obj {
      // JSON-doc convention — skip `_comment` / `_notes` keys (same
      // rule the recipe / card loaders use).
      if key.starts_with('_') {
        continue;
      }
      let body = body_val.as_object().ok_or_else(|| {
        format!("{}: blueprint {:?}: body must be an object", filename, key)
      })?;
      let blueprint_card = body
        .get("blueprint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
          format!(
            "{}: blueprint {:?}: missing string field `blueprint`",
            filename, key
          )
        })?;
      let card = body
        .get("card")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
          format!(
            "{}: blueprint {:?}: missing string field `card`",
            filename, key
          )
        })?;
      let blueprint_packed = find_packed_by_key(blueprint_card)
        .map_err(|e| format!("{}: blueprint {:?}: {}", filename, key, e))?
        .ok_or_else(|| {
          format!(
            "{}: blueprint {:?}: unknown `blueprint` card key {:?} (not declared in any cards/data/*.json)",
            filename, key, blueprint_card
          )
        })?;
      let card_packed = find_packed_by_key(card)
        .map_err(|e| format!("{}: blueprint {:?}: {}", filename, key, e))?
        .ok_or_else(|| {
          format!(
            "{}: blueprint {:?}: unknown `card` key {:?} (not declared in any cards/data/*.json)",
            filename, key, card
          )
        })?;

      let stable_id = stable_ids.get(key).copied().ok_or_else(|| {
        format!(
          "{}: blueprint {:?} not in blueprints/id.json — run gen-ids.py",
          filename, key
        )
      })?;

      let bp = Blueprint {
        id: stable_id,
        key: key.clone(),
        blueprint_key: blueprint_card.to_string(),
        blueprint_packed_definition: blueprint_packed,
        card_key: card.to_string(),
        card_packed_definition: card_packed,
      };
      if by_id.insert(stable_id, bp).is_some() {
        return Err(format!(
          "{}: duplicate stable id {} for blueprint {:?}",
          filename, stable_id, key
        ));
      }
      by_key.insert(key.clone(), stable_id);
    }
  }

  let mut ordered: Vec<BlueprintId> = by_id.keys().copied().collect();
  ordered.sort();

  Ok(BlueprintRegistry { by_id, by_key, ordered })
}
