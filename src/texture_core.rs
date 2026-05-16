//! Texture registry.
//!
//! A texture is a render hint matched against cards by **aspect**: for
//! a given `card_type`, each entry pins an aspect name, and any card
//! of that type carrying the aspect renders with the entry's spec.
//! The texture's outer key (`wood`, `stone`, `flora`, …) IS the
//! aspect predicate — there's no separate `aspect` field. A card can
//! match multiple textures (one per aspect it carries); the renderer
//! typically picks one (random / by-state / by-seed).
//!
//! (The `card_category` axis was retired — see
//! `docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md`.)
//!
//! Textures are loaded by the content crate but interpreted only
//! client-side — the server never reads them.
//!
//! # Loading
//!
//! Source data lives in `<repo>/content/textures/`:
//!
//! - `textures/id.json` — stable id map produced by `gen-ids.py`.
//!   `{ "<card_type>": { "<aspect_name>": <int>, ... } }`. IDs are
//!   1-indexed `u16`; 0 is reserved as [`TEXTURE_NONE`].
//!
//! - `textures/data/**/*.json` — each file is a nested object:
//!   `{ "<card_type>": { "<aspect_name>": { ... } } }`. Auto-discovered
//!   by `build.rs` (no manual file registration).
//!
//! Per-texture spec fields:
//!
//! | Field | Type | Notes |
//! | --- | --- | --- |
//! | `object` | string | Renderer-side asset key (sprite name / atlas frame / etc.). Opaque to the content crate — passed through verbatim. |
//! | `size`   | u32    | Pixel size of the source asset at native resolution. |
//! | `scale`  | object | `{ "min": <f32>, "max": <f32> }`. Render-time random scale envelope. `0 ≤ min ≤ max`, both finite. |
//!
//! The texture's nesting `(card_type, aspect_name)` is validated at
//! registry build:
//!
//! - `card_type` resolves through
//!   [`crate::definition_core::card_type_ids`]. Unknown types are
//!   silently skipped (same discipline as the cards loader).
//! - `aspect_name` resolves through
//!   [`crate::definition_core::aspect_id`]. Unknown aspects are a
//!   **hard build error** — same rule cards follow for their `aspects`
//!   map.
//!
//! # Lookups
//!
//! - [`texture`] — by stable [`TextureId`].
//! - [`find_texture`] — by `(card_type, aspect_name)`.
//! - [`textures_for_card`] — every texture whose type matches the
//!   card's `card_type` AND whose aspect the card carries. The
//!   typical renderer entry point.
//! - [`textures`] — every texture, ordered by stable id.
//!
//! # Failure mode
//!
//! Same `OnceLock<Result<Registry, String>>` pattern as the other
//! registries: a malformed file fails the build once and every
//! subsequent lookup returns the cached error.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::definition_core::{
  aspect_id, card_type_ids, decode_definition, AspectId,
};
use crate::embedded_data::TEXTURES_FILES;

pub type TextureId = u16;

/// Sentinel id meaning "no texture". Texture IDs are 1-indexed in
/// `textures/id.json`.
pub const TEXTURE_NONE: TextureId = 0;

const TEXTURE_IDS_JSON: &str = include_str!("../textures/id.json");

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureScale {
  pub min: f32,
  pub max: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureDefinition {
  /// Stable id from `textures/id.json`. 1-indexed; 0 is reserved as
  /// [`TEXTURE_NONE`].
  pub id: TextureId,
  /// Card-type id this texture applies to.
  pub card_type: u8,
  /// Aspect id this texture matches. Cards in `card_type` carrying
  /// this aspect render with this texture.
  pub aspect_id: AspectId,
  /// Aspect name from JSON (the texture's outer key), kept for
  /// diagnostics and for renderer-side locale lookups if needed.
  pub aspect_name: String,
  /// Renderer-side asset key copied through from the JSON `object`
  /// field. Opaque to the content crate — the renderer interprets it
  /// (sprite name, atlas frame, mesh handle, etc.).
  pub object: String,
  /// Native pixel size of the source asset.
  pub size: u32,
  /// Render-time random scale envelope.
  pub scale: TextureScale,
}

struct TextureRegistry {
  by_id: BTreeMap<TextureId, TextureDefinition>,
  /// `(card_type, aspect_id)` → stable id.
  by_path: BTreeMap<(u8, AspectId), TextureId>,
  /// `card_type` → texture ids in stable-id order. Used by
  /// [`textures_for_card`] to scope its aspect-membership filter
  /// to the relevant bucket.
  by_type: BTreeMap<u8, Vec<TextureId>>,
  /// Sorted-by-id list, used by [`textures`].
  ordered_ids: Vec<TextureId>,
}

static TEXTURES: OnceLock<Result<TextureRegistry, String>> = OnceLock::new();

fn textures_registry() -> Result<&'static TextureRegistry, String> {
  TEXTURES.get_or_init(build_textures).as_ref().map_err(|e| e.clone())
}

/// Look up a texture by its stable id. `Ok(None)` for [`TEXTURE_NONE`]
/// or for ids not present in the registry; `Err` on registry-build
/// failure.
pub fn texture(id: TextureId) -> Result<Option<&'static TextureDefinition>, String> {
  if id == TEXTURE_NONE {
    return Ok(None);
  }
  Ok(textures_registry()?.by_id.get(&id))
}

/// Find a texture by its `(card_type, aspect_name)` pair. `Ok(None)`
/// if no such texture is registered or the aspect name is unknown;
/// `Err` on registry-build failure.
pub fn find_texture(
  card_type: u8,
  aspect_name: &str,
) -> Result<Option<&'static TextureDefinition>, String> {
  let Some(aid) = aspect_id(aspect_name)? else {
    return Ok(None);
  };
  let registry = textures_registry()?;
  let Some(&id) = registry.by_path.get(&(card_type, aid)) else {
    return Ok(None);
  };
  Ok(registry.by_id.get(&id))
}

/// Every texture matching the card described by `packed_definition`:
/// same `card_type` bucket, and the card carries the texture's
/// aspect. Returned in stable-id order. Empty vec when the card is
/// unknown or no texture in its bucket matches an aspect it
/// carries. `Err` on registry-build failure.
///
/// Typical renderer use: ask for all textures painting a given card,
/// then pick one (random / by-state / by-seed) and draw it.
pub fn textures_for_card(
  packed_definition: u16,
) -> Result<Vec<&'static TextureDefinition>, String> {
  let Some(def) = decode_definition(packed_definition)? else {
    return Ok(Vec::new());
  };
  let registry = textures_registry()?;
  let Some(ids) = registry.by_type.get(&def.card_type) else {
    return Ok(Vec::new());
  };
  let mut out: Vec<&'static TextureDefinition> = Vec::new();
  for &id in ids {
    let Some(tex) = registry.by_id.get(&id) else { continue };
    if def.aspects.iter().any(|(a, _)| *a == tex.aspect_id) {
      out.push(tex);
    }
  }
  Ok(out)
}

/// All registered textures, ordered by stable id. Empty when the
/// `textures/data/` tree contains nothing.
pub fn textures() -> Result<Vec<&'static TextureDefinition>, String> {
  let registry = textures_registry()?;
  Ok(
    registry
      .ordered_ids
      .iter()
      .filter_map(|id| registry.by_id.get(id))
      .collect(),
  )
}

fn build_textures() -> Result<TextureRegistry, String> {
  let type_ids = card_type_ids()?;

  // 1. Stable id map. Shape: `{ <type>: { <aspect_name>: id } }`
  // (category retired — see docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md).
  let id_root: Value = serde_json::from_str(TEXTURE_IDS_JSON)
    .map_err(|e| format!("textures/id.json: parse failed: {}", e))?;
  let id_obj = id_root
    .as_object()
    .ok_or_else(|| "textures/id.json: top-level not an object".to_string())?;
  let mut stable_ids: BTreeMap<(String, String), TextureId> = BTreeMap::new();
  for (type_name, type_val) in id_obj {
    let type_obj = type_val.as_object().ok_or_else(|| {
      format!("textures/id.json: entry for type {:?} not an object", type_name)
    })?;
    for (aspect_name, id_value) in type_obj {
      let n = id_value.as_u64().ok_or_else(|| {
        format!(
          "textures/id.json: id for {:?}/{:?} not an integer",
          type_name, aspect_name
        )
      })?;
      if n == 0 || n > TextureId::MAX as u64 {
        return Err(format!(
          "textures/id.json: id {} for {:?}/{:?} out of range (1..={})",
          n,
          type_name,
          aspect_name,
          TextureId::MAX,
        ));
      }
      stable_ids.insert(
        (type_name.clone(), aspect_name.clone()),
        n as TextureId,
      );
    }
  }

  // 2. Parse each data file. Unknown types are silently skipped —
  // same discipline as cards loader. Unknown aspect names ARE a
  // hard error (matches the rule for cards' own `aspects` map).
  let mut by_id: BTreeMap<TextureId, TextureDefinition> = BTreeMap::new();
  let mut by_path: BTreeMap<(u8, AspectId), TextureId> = BTreeMap::new();
  let mut by_type: BTreeMap<u8, Vec<TextureId>> = BTreeMap::new();

  for (filename, content) in TEXTURES_FILES {
    let parsed: Value = serde_json::from_str(content)
      .map_err(|e| format!("{}: parse failed: {}", filename, e))?;
    let root = parsed.as_object().ok_or_else(|| {
      format!("{}: top-level must be an object keyed by card_type", filename)
    })?;

    for (type_name, textures_val) in root {
      let Some(&card_type) = type_ids.get(type_name) else { continue };
      let textures_obj = textures_val.as_object().ok_or_else(|| {
        format!("{}: type {:?}: value not an object", filename, type_name)
      })?;

      for (aspect_name, value) in textures_obj {
        let stable_id = stable_ids
          .get(&(type_name.clone(), aspect_name.clone()))
          .copied()
          .ok_or_else(|| {
            format!(
              "{}: texture {:?} (type {:?}) not found in textures/id.json — run gen-ids.py",
              filename, aspect_name, type_name
            )
          })?;
        let aid = aspect_id(aspect_name)
          .map_err(|e| format!("{}: texture {:?}: {}", filename, aspect_name, e))?
          .ok_or_else(|| {
            format!(
              "{}: texture {:?} (type {:?}): unknown aspect (not declared in aspects.json)",
              filename, aspect_name, type_name
            )
          })?;
        let definition =
          parse_texture(filename, value, stable_id, card_type, aid, aspect_name)?;
        if by_id.insert(stable_id, definition).is_some() {
          return Err(format!(
            "{}: duplicate stable id {} for texture {:?}/{:?}",
            filename, stable_id, type_name, aspect_name
          ));
        }
        by_path.insert((card_type, aid), stable_id);
        by_type.entry(card_type).or_default().push(stable_id);
      }
    }
  }

  // Sort each bucket's texture-id list deterministically.
  for ids in by_type.values_mut() {
    ids.sort();
  }

  let ordered_ids: Vec<TextureId> = by_id.keys().copied().collect();

  Ok(TextureRegistry { by_id, by_path, by_type, ordered_ids })
}

fn parse_texture(
  filename: &str,
  value: &Value,
  id: TextureId,
  card_type: u8,
  aspect_id: AspectId,
  aspect_name: &str,
) -> Result<TextureDefinition, String> {
  let obj = value
    .as_object()
    .ok_or_else(|| format!("{}: texture {}: spec not an object", filename, aspect_name))?;

  let object = obj
    .get("object")
    .and_then(Value::as_str)
    .ok_or_else(|| {
      format!(
        "{}: texture {}: missing or non-string 'object' field",
        filename, aspect_name
      )
    })?
    .to_string();

  let size_u64 = obj
    .get("size")
    .and_then(Value::as_u64)
    .ok_or_else(|| {
      format!(
        "{}: texture {}: missing or non-integer 'size'",
        filename, aspect_name
      )
    })?;
  if size_u64 > u32::MAX as u64 {
    return Err(format!(
      "{}: texture {}: size {} exceeds u32 max",
      filename, aspect_name, size_u64
    ));
  }

  let scale_val = obj.get("scale").ok_or_else(|| {
    format!("{}: texture {}: missing 'scale' field", filename, aspect_name)
  })?;
  let scale_obj = scale_val.as_object().ok_or_else(|| {
    format!(
      "{}: texture {}: 'scale' must be an object",
      filename, aspect_name
    )
  })?;
  let min = scale_obj
    .get("min")
    .and_then(Value::as_f64)
    .ok_or_else(|| {
      format!(
        "{}: texture {}: scale.min missing or not a number",
        filename, aspect_name
      )
    })? as f32;
  let max = scale_obj
    .get("max")
    .and_then(Value::as_f64)
    .ok_or_else(|| {
      format!(
        "{}: texture {}: scale.max missing or not a number",
        filename, aspect_name
      )
    })? as f32;
  if !min.is_finite() || !max.is_finite() {
    return Err(format!(
      "{}: texture {}: scale.min / scale.max must be finite (got min={}, max={})",
      filename, aspect_name, min, max
    ));
  }
  if min < 0.0 {
    return Err(format!(
      "{}: texture {}: scale.min {} must be non-negative",
      filename, aspect_name, min
    ));
  }
  if max < min {
    return Err(format!(
      "{}: texture {}: scale.max {} less than scale.min {}",
      filename, aspect_name, max, min
    ));
  }

  Ok(TextureDefinition {
    id,
    card_type,
    aspect_id,
    aspect_name: aspect_name.to_string(),
    object,
    size: size_u64 as u32,
    scale: TextureScale { min, max },
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  // Forces the lazy registry to build, surfacing any schema mismatch
  // (unknown aspect, malformed scale, etc.) at `cargo test` time
  // instead of waiting for a runtime lookup.
  #[test]
  fn registry_builds() {
    textures_registry().expect("texture registry should build clean");
  }
}
