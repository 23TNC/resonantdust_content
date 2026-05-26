//! Texture registry.
//!
//! Render metadata for **objects** (sprite packs). The card-side art
//! lookup resolves through this registry by object name; an object's
//! `size` / `scale` / `anchor` live in `objects.json`, a flat catalog
//! separate from the aspect taxonomy. The pack-folder on disk is named
//! `master/<name>/` — pack name and object name are the same string.
//!
//! Pre-split this module walked `aspects.json` and treated entries with
//! a `size` field as renderable, conflating the recipe taxonomy with
//! the sprite catalog. Objects are no longer aspects — `objects.json`
//! is the source of truth for renderable things, `aspects.json` is
//! purely recipe-side. Where a card carries an aspect whose name
//! matches an object's name (`pine`, `stone`, `soul`, …) the renderer
//! auto-pairs them by string match.
//!
//! Textures are loaded by the content crate but interpreted only
//! client-side — the server never reads them.
//!
//! # Lookups
//!
//! - [`texture`] — by stable [`TextureId`].
//! - [`find_texture`] — by object name.
//! - [`textures_for_card`] — every renderable object whose name matches
//!   an aspect the card carries (returned in stable-id order). The
//!   typical renderer entry point for aspect-driven tile decoration.
//! - [`textures`] — every registered object, ordered by stable id.
//!
//! # ID stability
//!
//! IDs are assigned at registry-build time in the order objects appear
//! in `objects.json`. Reordering or inserting objects changes the id of
//! later entries. Textures are client-only — neither persisted nor on
//! the wire — so id churn is acceptable.
//!
//! # Failure mode
//!
//! Same `OnceLock<Result<Registry, String>>` pattern as the other
//! registries: a malformed source fails the build once and every
//! subsequent lookup returns the cached error.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::definition_core::{
  aspect, decode_definition, RenderAnchor, RenderScale, DEFAULT_RENDER_ANCHOR,
  DEFAULT_RENDER_SCALE,
};

pub type TextureId = u16;

/// Sentinel id meaning "no texture". Texture IDs are 1-indexed.
pub const TEXTURE_NONE: TextureId = 0;

// Re-exports to keep historical import sites compiling. The render
// types live on `definition_core` because aspects historically carried
// them; objects now use the same shapes.
pub use crate::definition_core::{RenderAnchor as TextureAnchor, RenderScale as TextureScale};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureDefinition {
  /// Stable id assigned at registry-build time. 1-indexed; 0 is
  /// reserved as [`TEXTURE_NONE`].
  pub id: TextureId,
  /// Object name — same string the renderer uses as the pack-folder
  /// name (`master/<name>/`). Cards reference this via
  /// `object: { name }` and `texture: { name }`; tile stock carrying
  /// an aspect of the same name also resolves through here.
  pub name: String,
  /// Native pixel size of the source asset.
  pub size: u32,
  /// Render-time random scale envelope.
  pub scale: TextureScale,
  /// Sprite pivot point.
  pub anchor: TextureAnchor,
  /// Symbolic-name → on-disk `<N>.png` index lookup table.
  /// Cards reference textures by name (`object.index: "log"`); the
  /// parser resolves the name through this map at parse time so card
  /// defs never carry raw integer indices. Re-numbering sprites in
  /// the pack only touches this map. Empty when the pack omits the
  /// optional `textures` field in objects.json (cards then can't pin
  /// an index and fall back to the runtime pseudo-random pick).
  pub textures: BTreeMap<String, u32>,
}

struct TextureRegistry {
  by_id: BTreeMap<TextureId, TextureDefinition>,
  /// Object name → stable texture id. Direct one-arg lookup.
  by_name: BTreeMap<String, TextureId>,
  /// Stable-id order — used by [`textures`] and by
  /// [`textures_for_card`] when intersecting with a card's aspects.
  ordered_ids: Vec<TextureId>,
}

const OBJECTS_JSON: &str = include_str!("../cards/objects.json");
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

/// Resolve a symbolic texture name within an object's pack to the
/// on-disk `<N>.png` index. Returns `Ok(Some(idx))` on hit,
/// `Ok(None)` when the pack or symbol isn't registered, and `Err`
/// on registry-build failure. Card-def parsing uses this to bake
/// `object.index: "<symbol>"` references down to integer indices.
pub fn texture_index(pack: &str, symbol: &str) -> Result<Option<u32>, String> {
  let Some(def) = find_texture(pack)? else {
    return Ok(None);
  };
  Ok(def.textures.get(symbol).copied())
}

/// Find a texture by object name. `Ok(None)` when no object with that
/// name is registered; `Err` on registry-build failure.
pub fn find_texture(name: &str) -> Result<Option<&'static TextureDefinition>, String> {
  let registry = textures_registry()?;
  let Some(&id) = registry.by_name.get(name) else {
    return Ok(None);
  };
  Ok(registry.by_id.get(&id))
}

/// Every texture whose object name matches an aspect the card
/// described by `packed_definition` carries. Returned in stable-id
/// order. Empty vec when the card is unknown or none of its aspect
/// names have a matching object. `Err` on registry-build failure.
///
/// Typical renderer use: ask for all renderable things on a given
/// card (tile decoration, etc.), then pick / arrange / draw them.
pub fn textures_for_card(
  packed_definition: u16,
) -> Result<Vec<&'static TextureDefinition>, String> {
  let Some(def) = decode_definition(packed_definition)? else {
    return Ok(Vec::new());
  };
  let registry = textures_registry()?;
  let mut out: Vec<&'static TextureDefinition> = Vec::new();
  for (aspect_id_carried, _) in &def.aspects {
    let Some(asp) = aspect(*aspect_id_carried)? else { continue };
    if let Some(&tid) = registry.by_name.get(&asp.name) {
      if let Some(tex) = registry.by_id.get(&tid) {
        out.push(tex);
      }
    }
  }
  out.sort_by_key(|t| t.id);
  Ok(out)
}

/// All registered textures, ordered by stable id.
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
  let root: Value = serde_json::from_str(OBJECTS_JSON)
    .map_err(|e| format!("objects.json: parse failed: {}", e))?;
  let root = root
    .as_object()
    .ok_or_else(|| "objects.json: top-level not an object".to_string())?;

  let mut by_id: BTreeMap<TextureId, TextureDefinition> = BTreeMap::new();
  let mut by_name: BTreeMap<String, TextureId> = BTreeMap::new();
  let mut ordered_ids: Vec<TextureId> = Vec::new();
  let mut next_id: u32 = 1;

  for (name, entry_value) in root {
    if name.starts_with('_') {
      continue;
    }
    let entry = entry_value.as_object().ok_or_else(|| {
      format!(
        "objects.json: object {:?} not an object (expected {{\"size\": ..., ...}})",
        name
      )
    })?;

    let size = entry
      .get("size")
      .and_then(Value::as_u64)
      .ok_or_else(|| {
        format!(
          "objects.json: object {:?} missing 'size' (must be a non-negative integer)",
          name
        )
      })?;
    if size > u32::MAX as u64 {
      return Err(format!(
        "objects.json: object {:?} 'size' {} exceeds u32 max",
        name, size
      ));
    }
    let size = size as u32;

    let scale = match entry.get("scale") {
      None => DEFAULT_RENDER_SCALE,
      Some(v) => {
        let obj = v.as_object().ok_or_else(|| {
          format!("objects.json: object {:?} 'scale' must be an object", name)
        })?;
        let min = obj.get("min").and_then(Value::as_f64).ok_or_else(|| {
          format!(
            "objects.json: object {:?} scale.min missing or not a number",
            name
          )
        })? as f32;
        let max = obj.get("max").and_then(Value::as_f64).ok_or_else(|| {
          format!(
            "objects.json: object {:?} scale.max missing or not a number",
            name
          )
        })? as f32;
        if !min.is_finite() || !max.is_finite() {
          return Err(format!(
            "objects.json: object {:?} scale.min / scale.max must be finite (got min={}, max={})",
            name, min, max
          ));
        }
        if min < 0.0 {
          return Err(format!(
            "objects.json: object {:?} scale.min {} must be non-negative",
            name, min
          ));
        }
        if max < min {
          return Err(format!(
            "objects.json: object {:?} scale.max {} less than scale.min {}",
            name, max, min
          ));
        }
        RenderScale { min, max }
      }
    };

    let textures = match entry.get("textures") {
      None => BTreeMap::new(),
      Some(v) => {
        let obj = v.as_object().ok_or_else(|| {
          format!(
            "objects.json: object {:?} 'textures' must be an object {{ <symbol>: <index>, ... }}",
            name
          )
        })?;
        let mut map: BTreeMap<String, u32> = BTreeMap::new();
        for (sym, idx_v) in obj {
          let n = idx_v.as_u64().ok_or_else(|| {
            format!(
              "objects.json: object {:?} textures[{:?}] must be a non-negative integer",
              name, sym
            )
          })?;
          if n > u32::MAX as u64 {
            return Err(format!(
              "objects.json: object {:?} textures[{:?}] {} exceeds u32 max",
              name, sym, n
            ));
          }
          map.insert(sym.clone(), n as u32);
        }
        map
      }
    };

    let anchor = match entry.get("anchor") {
      None => DEFAULT_RENDER_ANCHOR,
      Some(v) => {
        let obj = v.as_object().ok_or_else(|| {
          format!(
            "objects.json: object {:?} 'anchor' must be {{ \"x\": <num>, \"y\": <num> }}",
            name
          )
        })?;
        let ax = obj.get("x").and_then(Value::as_f64).ok_or_else(|| {
          format!(
            "objects.json: object {:?} anchor.x missing or not a number",
            name
          )
        })? as f32;
        let ay = obj.get("y").and_then(Value::as_f64).ok_or_else(|| {
          format!(
            "objects.json: object {:?} anchor.y missing or not a number",
            name
          )
        })? as f32;
        if !ax.is_finite() || !ay.is_finite() {
          return Err(format!(
            "objects.json: object {:?} anchor.x / anchor.y must be finite (got x={}, y={})",
            name, ax, ay
          ));
        }
        RenderAnchor { x: ax, y: ay }
      }
    };

    if by_name.contains_key(name) {
      return Err(format!(
        "objects.json: object {:?} declared more than once",
        name
      ));
    }

    if next_id > TextureId::MAX as u32 {
      return Err(format!(
        "objects.json: more than {} objects (texture id overflow)",
        TextureId::MAX,
      ));
    }
    let id = next_id as TextureId;
    next_id += 1;

    by_name.insert(name.clone(), id);
    by_id.insert(
      id,
      TextureDefinition {
        id,
        name: name.clone(),
        size,
        scale,
        anchor,
        textures,
      },
    );
    ordered_ids.push(id);
  }

  Ok(TextureRegistry { by_id, by_name, ordered_ids })
}

#[cfg(test)]
mod tests {
  use super::*;

  // Forces the lazy registry to build, surfacing any schema mismatch
  // (missing `size`, malformed scale, etc.) at `cargo test` time
  // instead of waiting for a runtime lookup.
  #[test]
  fn registry_builds() {
    textures_registry().expect("texture registry should build clean");
  }
}
