//! Starter-pack registry.
//!
//! A starter pack is a named bundle of cards spawned for a player at
//! game start, scoped to a soul. Each pack has a stable id from
//! `starter_packs/id.json` plus a `(soul, pack_id)` path mirroring the
//! recipe / card scoping conventions.
//!
//! # Loading
//!
//! Source data lives in `<repo>/content/starter_packs/`:
//!
//! - `starter_packs/id.json` — stable id map produced by `gen-ids.py`.
//!   Format: `{ "<soul>": { "<pack_id>": <int>, ... }, ... }`.
//! - `starter_packs/data/**/*.json` — each file is either a top-level
//!   `{ "<soul>": { "<pack_id>": { "<card_key>": <count>, ... } } }`
//!   object or an array of such objects.
//!
//! Card keys in pack contents are resolved to `packed_definition` at
//! registry build time via [`crate::definition_core::find_packed_by_key`].
//! The `soul` key is also validated against the card registry — a typo
//! becomes a stored registry-build error rather than a runtime spawn
//! failure.
//!
//! # Failure mode
//!
//! Same `OnceLock<Result<Registry, String>>` pattern as
//! `definition_core` / `recipe_core`: a malformed file fails the
//! build once and every subsequent lookup returns the cached error.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::definition_core::find_packed_by_key;
use crate::embedded_data::STARTER_PACKS_FILES;

pub type StarterPackId = u16;

/// Sentinel id meaning "no starter pack". Pack IDs are 1-indexed in
/// `starter_packs/id.json`.
pub const STARTER_PACK_NONE: StarterPackId = 0;

const STARTER_PACK_IDS_JSON: &str = include_str!("../starter_packs/id.json");

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StarterPackItem {
  /// Card key from the source JSON, e.g. `"axe"`.
  pub card_key: String,
  /// `packed_definition` resolved via `find_packed_by_key`.
  pub packed_definition: u16,
  /// Number of copies of this card to spawn when the pack is chosen.
  pub count: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StarterPack {
  /// Stable id from `starter_packs/id.json`. 1-indexed; 0 is reserved
  /// as `STARTER_PACK_NONE`.
  pub id: StarterPackId,
  /// Soul card key this pack belongs to, e.g. `"human"`. Validated as
  /// a known card key at registry build time.
  pub soul: String,
  /// Pack id within the soul, e.g. `"default"`.
  pub pack_id: String,
  /// Cards spawned when this pack is chosen.
  pub contents: Vec<StarterPackItem>,
}

struct StarterPackRegistry {
  by_id: BTreeMap<StarterPackId, StarterPack>,
  /// `(soul, pack_id)` → stable id.
  by_path: BTreeMap<(String, String), StarterPackId>,
  /// Soul → its packs' stable ids in ascending order.
  by_soul: BTreeMap<String, Vec<StarterPackId>>,
}

static STARTER_PACKS: OnceLock<Result<StarterPackRegistry, String>> = OnceLock::new();

fn starter_packs_registry() -> Result<&'static StarterPackRegistry, String> {
  STARTER_PACKS.get_or_init(build_starter_packs).as_ref().map_err(|e| e.clone())
}

/// Look up a starter pack by its stable id. `Ok(None)` for
/// `STARTER_PACK_NONE` or for ids not present in the registry; `Err`
/// on registry-build failure.
pub fn starter_pack(id: StarterPackId) -> Result<Option<&'static StarterPack>, String> {
  if id == STARTER_PACK_NONE {
    return Ok(None);
  }
  Ok(starter_packs_registry()?.by_id.get(&id))
}

/// Find a starter pack by `(soul, pack_id)`. `Ok(None)` if no such
/// pack is registered; `Err` on registry-build failure.
pub fn find_starter_pack(
  soul: &str,
  pack_id: &str,
) -> Result<Option<&'static StarterPack>, String> {
  let registry = starter_packs_registry()?;
  let key = (soul.to_string(), pack_id.to_string());
  let Some(&id) = registry.by_path.get(&key) else {
    return Ok(None);
  };
  Ok(registry.by_id.get(&id))
}

/// All starter packs declared for the given soul, ordered by stable id.
/// Empty vec if the soul has no packs.
pub fn starter_packs_for_soul(soul: &str) -> Result<Vec<&'static StarterPack>, String> {
  let registry = starter_packs_registry()?;
  let Some(ids) = registry.by_soul.get(soul) else {
    return Ok(Vec::new());
  };
  Ok(ids.iter().filter_map(|id| registry.by_id.get(id)).collect())
}

fn build_starter_packs() -> Result<StarterPackRegistry, String> {
  // 1. Load stable id map: { "<soul>": { "<pack_id>": <int> } }.
  let id_root: Value = serde_json::from_str(STARTER_PACK_IDS_JSON)
    .map_err(|e| format!("starter_packs/id.json: parse failed: {}", e))?;
  let id_obj = id_root.as_object().ok_or_else(|| {
    "starter_packs/id.json: top-level not an object".to_string()
  })?;
  let mut stable_ids: BTreeMap<(String, String), StarterPackId> = BTreeMap::new();
  for (soul, packs_val) in id_obj {
    let packs_obj = packs_val.as_object().ok_or_else(|| {
      format!("starter_packs/id.json: entry for soul {:?} not an object", soul)
    })?;
    for (pack_id, id_value) in packs_obj {
      let n = id_value.as_u64().ok_or_else(|| {
        format!(
          "starter_packs/id.json: id for {:?}/{:?} not an integer",
          soul, pack_id
        )
      })?;
      if n == 0 || n > StarterPackId::MAX as u64 {
        return Err(format!(
          "starter_packs/id.json: id {} for {:?}/{:?} out of range (1..={})",
          n, soul, pack_id, StarterPackId::MAX
        ));
      }
      stable_ids.insert((soul.clone(), pack_id.clone()), n as StarterPackId);
    }
  }

  // 2. Parse each data file.
  let mut by_id: BTreeMap<StarterPackId, StarterPack> = BTreeMap::new();
  let mut by_path: BTreeMap<(String, String), StarterPackId> = BTreeMap::new();
  let mut by_soul: BTreeMap<String, Vec<StarterPackId>> = BTreeMap::new();

  for (filename, content) in STARTER_PACKS_FILES {
    let parsed: Value = serde_json::from_str(content)
      .map_err(|e| format!("{}: parse failed: {}", filename, e))?;
    // Accept either a single { soul: {...} } object or an array of such
    // objects, mirroring the card / recipe loaders.
    let owned_singleton;
    let entries: &[Value] = match &parsed {
      Value::Array(v) => v.as_slice(),
      Value::Object(_) => {
        owned_singleton = [parsed.clone()];
        &owned_singleton
      }
      _ => {
        return Err(format!(
          "{}: top-level must be an object or array of objects",
          filename
        ));
      }
    };

    for entry in entries {
      let entry_obj = entry
        .as_object()
        .ok_or_else(|| format!("{}: entry not an object", filename))?;
      for (soul, packs_val) in entry_obj {
        // Validate soul is a known card key — catches typos at build time.
        if find_packed_by_key(soul)
          .map_err(|e| format!("{}: soul {:?}: {}", filename, soul, e))?
          .is_none()
        {
          return Err(format!(
            "{}: soul {:?} is not a known card key (not declared in any cards/data/*.json)",
            filename, soul
          ));
        }

        let packs_obj = packs_val.as_object().ok_or_else(|| {
          format!("{}: soul {:?}: pack map not an object", filename, soul)
        })?;

        for (pack_id, contents_val) in packs_obj {
          let stable_id = stable_ids
            .get(&(soul.clone(), pack_id.clone()))
            .copied()
            .ok_or_else(|| {
              format!(
                "{}: pack {:?}/{:?} not in starter_packs/id.json — run gen-ids.py",
                filename, soul, pack_id
              )
            })?;

          let contents_obj = contents_val.as_object().ok_or_else(|| {
            format!(
              "{}: pack {:?}/{:?}: contents not an object",
              filename, soul, pack_id
            )
          })?;

          let mut contents: Vec<StarterPackItem> = Vec::with_capacity(contents_obj.len());
          for (card_key, count_val) in contents_obj {
            let packed = find_packed_by_key(card_key)
              .map_err(|e| {
                format!("{}: pack {:?}/{:?}: card {:?}: {}", filename, soul, pack_id, card_key, e)
              })?
              .ok_or_else(|| {
                format!(
                  "{}: pack {:?}/{:?}: unknown card key {:?}",
                  filename, soul, pack_id, card_key
                )
              })?;
            let count = count_val.as_u64().ok_or_else(|| {
              format!(
                "{}: pack {:?}/{:?}: count for card {:?} not a non-negative integer",
                filename, soul, pack_id, card_key
              )
            })?;
            if count > u32::MAX as u64 {
              return Err(format!(
                "{}: pack {:?}/{:?}: count {} for card {:?} exceeds u32 max",
                filename, soul, pack_id, count, card_key
              ));
            }
            contents.push(StarterPackItem {
              card_key: card_key.clone(),
              packed_definition: packed,
              count: count as u32,
            });
          }

          let pack = StarterPack {
            id: stable_id,
            soul: soul.clone(),
            pack_id: pack_id.clone(),
            contents,
          };
          if by_id.insert(stable_id, pack).is_some() {
            return Err(format!(
              "{}: duplicate stable id {} for pack {:?}/{:?}",
              filename, stable_id, soul, pack_id
            ));
          }
          by_path.insert((soul.clone(), pack_id.clone()), stable_id);
          by_soul.entry(soul.clone()).or_default().push(stable_id);
        }
      }
    }
  }

  // Sort each soul's pack list by stable id so iteration order is
  // deterministic regardless of file traversal order.
  for ids in by_soul.values_mut() {
    ids.sort();
  }

  Ok(StarterPackRegistry { by_id, by_path, by_soul })
}
