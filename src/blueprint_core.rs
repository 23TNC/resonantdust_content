//! Blueprint registry — two scopes (soul + player) sharing one
//! schema, both indexed in this module.
//!
//! A blueprint is a named "recipe to build a card" — at this point each
//! entry just carries a target `card_id` (the card the blueprint
//! produces) and the renderable blueprint-card key (the visual the
//! UI draws in the discovered-blueprints panel). The schema will
//! grow later to include build cost, prerequisites, output stats,
//! etc.; this initial pass establishes the `(key → stable id →
//! resolved card)` plumbing.
//!
//! # Scopes
//!
//! - [`BlueprintScope::Soul`]: soul-bound build plans. Discovery bit
//!   lives on `SoulPrivate.blueprints_0`; place cap is the soul's
//!   `aspects.builder` value; spawned cards use the `blueprint`
//!   card_type (id=1).
//! - [`BlueprintScope::Player`]: account-wide build plans. Discovery
//!   bit lives on `PlayerProfile.blueprints_0`; place cap is the
//!   `PlayerProfile.blueprint_info.max` nibble; spawned cards use
//!   the `player_blueprint` card_type (id=3).
//!
//! Each scope has its own id namespace (1..=u16) so def_ids can't
//! collide — soul-blueprint 1 and player-blueprint 1 are unrelated
//! and resolve through different `find_blueprint(scope, key)`
//! lookups. The two registries share this module so the loader code
//! only exists once.
//!
//! # Loading
//!
//! Soul scope reads from `content/blueprints/{id.json,data/*.json}`.
//! Player scope reads from `content/player_blueprints/{...}`.
//! Both scopes use the same body schema:
//!
//! - `<scope>/id.json` — `{ "<key>": <int>, ... }` (flat namespace,
//!   produced by `gen-ids.py`).
//! - `<scope>/data/**/*.json` — each file is a top-level
//!   `{ "<key>": { "blueprint": "<bp_card_key>", "card":
//!   "<out_card_key>" } }` object. Additional body fields are
//!   tolerated and ignored.
//!
//! Both `blueprint` + `card` keys resolve to `packed_definition` at
//! registry build time via [`crate::definition_core::find_packed_by_key`]
//! — an unknown key becomes a stored registry-build error rather
//! than a runtime spawn failure.
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
use crate::embedded_data::{BLUEPRINTS_FILES, PLAYER_BLUEPRINTS_FILES};

pub type BlueprintId = u16;

/// Sentinel id meaning "no blueprint". Blueprint IDs are 1-indexed in
/// `<scope>/id.json`. Shared across scopes — both registries reserve 0.
pub const BLUEPRINT_NONE: BlueprintId = 0;

/// Which scope a blueprint belongs to. Determines the source
/// directory, the storage location of the discovery bitfield, and
/// the spawned card's `card_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BlueprintScope {
  Soul,
  Player,
}

const SOUL_BLUEPRINT_IDS_JSON: &str = include_str!("../blueprints/id.json");
const PLAYER_BLUEPRINT_IDS_JSON: &str = include_str!("../player_blueprints/id.json");

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blueprint {
  /// Which scope this blueprint belongs to. Soul-scope ids and
  /// player-scope ids live in independent namespaces — comparing
  /// ids across scopes is meaningless without this field.
  pub scope: BlueprintScope,
  /// Stable id from `<scope>/id.json`. 1-indexed; 0 is reserved as
  /// `BLUEPRINT_NONE`.
  pub id: BlueprintId,
  /// Blueprint key from the source JSON, e.g. `"nd_furnace"`.
  pub key: String,
  /// Blueprint *card* key — the card visual the UI draws when this
  /// blueprint is discovered. Sourced from the `"blueprint"` field.
  /// Validated as a known card key at registry build time.
  pub blueprint_key: String,
  /// `packed_definition` for `blueprint_key`, resolved via
  /// [`find_packed_by_key`].
  pub blueprint_packed_definition: u16,
  /// Output card key — the card produced when the blueprint is
  /// eventually built in-world. Sourced from the `"card"` field.
  pub card_key: String,
  /// `packed_definition` for `card_key`.
  pub card_packed_definition: u16,
}

struct ScopeRegistry {
  by_id: BTreeMap<BlueprintId, Blueprint>,
  by_key: BTreeMap<String, BlueprintId>,
  /// Stable-id-sorted list for deterministic iteration (UI rendering,
  /// catalog enumeration).
  ordered: Vec<BlueprintId>,
}

static SOUL_BLUEPRINTS: OnceLock<Result<ScopeRegistry, String>> = OnceLock::new();
static PLAYER_BLUEPRINTS: OnceLock<Result<ScopeRegistry, String>> = OnceLock::new();

fn registry(scope: BlueprintScope) -> Result<&'static ScopeRegistry, String> {
  let slot = match scope {
    BlueprintScope::Soul => &SOUL_BLUEPRINTS,
    BlueprintScope::Player => &PLAYER_BLUEPRINTS,
  };
  slot
    .get_or_init(|| build(scope))
    .as_ref()
    .map_err(|e| e.clone())
}

/// Look up a blueprint by its stable id within `scope`. `Ok(None)`
/// for `BLUEPRINT_NONE` or for ids not present in the registry;
/// `Err` on registry-build failure.
pub fn blueprint(
  scope: BlueprintScope,
  id: BlueprintId,
) -> Result<Option<&'static Blueprint>, String> {
  if id == BLUEPRINT_NONE {
    return Ok(None);
  }
  Ok(registry(scope)?.by_id.get(&id))
}

/// Look up a blueprint by its source-key within `scope` (e.g.
/// `("Soul", "nd_furnace")`). `Ok(None)` if no blueprint with that
/// key is registered in the scope; `Err` on registry-build failure.
pub fn find_blueprint(
  scope: BlueprintScope,
  key: &str,
) -> Result<Option<&'static Blueprint>, String> {
  let registry = registry(scope)?;
  let Some(&id) = registry.by_key.get(key) else {
    return Ok(None);
  };
  Ok(registry.by_id.get(&id))
}

/// Look up a blueprint by source-key across both scopes — Soul first,
/// then Player. Used by recipe outputs (`...owner.blueprint.unlock:
/// <key>`) where the author shouldn't need to spell out the scope:
/// the catalog entry knows which bitfield to write, so the executor
/// just asks "which blueprint is this key?" and dispatches by
/// `bp.scope`. `Ok(None)` if no scope has the key; `Err` on
/// registry-build failure. A future invariant could enforce
/// cross-scope key uniqueness; until then Soul wins on collision.
pub fn find_blueprint_any_scope(
  key: &str,
) -> Result<Option<&'static Blueprint>, String> {
  if let Some(bp) = find_blueprint(BlueprintScope::Soul, key)? {
    return Ok(Some(bp));
  }
  find_blueprint(BlueprintScope::Player, key)
}

/// Every registered blueprint in `scope` in stable-id order. Empty
/// vec when none are declared. `Err` on registry-build failure.
pub fn blueprints_all(scope: BlueprintScope) -> Result<Vec<&'static Blueprint>, String> {
  let registry = registry(scope)?;
  Ok(registry.ordered.iter().filter_map(|id| registry.by_id.get(id)).collect())
}

fn build(scope: BlueprintScope) -> Result<ScopeRegistry, String> {
  let (ids_json, data_files, scope_label) = match scope {
    BlueprintScope::Soul => (SOUL_BLUEPRINT_IDS_JSON, BLUEPRINTS_FILES, "blueprints"),
    BlueprintScope::Player => (
      PLAYER_BLUEPRINT_IDS_JSON,
      PLAYER_BLUEPRINTS_FILES,
      "player_blueprints",
    ),
  };

  // 1. Load stable id map: { "<blueprint_key>": <int> }.
  let id_root: Value = serde_json::from_str(ids_json)
    .map_err(|e| format!("{}/id.json: parse failed: {}", scope_label, e))?;
  let id_obj = id_root.as_object().ok_or_else(|| {
    format!("{}/id.json: top-level not an object", scope_label)
  })?;
  let mut stable_ids: BTreeMap<String, BlueprintId> = BTreeMap::new();
  for (key, id_value) in id_obj {
    let n = id_value.as_u64().ok_or_else(|| {
      format!("{}/id.json: id for {:?} not an integer", scope_label, key)
    })?;
    if n == 0 || n > BlueprintId::MAX as u64 {
      return Err(format!(
        "{}/id.json: id {} for {:?} out of range (1..={})",
        scope_label,
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

  for (filename, content) in data_files {
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
          "{}: blueprint {:?} not in {}/id.json — run gen-ids.py",
          filename, key, scope_label
        )
      })?;

      let bp = Blueprint {
        scope,
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

  Ok(ScopeRegistry { by_id, by_key, ordered })
}
