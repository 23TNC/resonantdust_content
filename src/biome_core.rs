//! Biome registry for procedural world generation.
//!
//! Loads `zones/biomes.json` and exposes the climate-envelope walk
//! `biome_for_climate(elev, temp, humidity, aether) -> BiomeId`.
//! Biomes are stored in declaration order — first matching envelope
//! wins, so the JSON puts specific biomes (mountain, desert) before
//! broad fallbacks (plains).
//!
//! Tile selection does NOT consult this module — that's a separate
//! pass in `world_gen.rs` driven by per-tile climate envelopes
//! (`climate.*` traits in `cards/traits.json`). Biome assignment here
//! exists for (a) the revert-on-consume tile lookup
//! (`world_gen::biome_for` → biome's `base_tile`) and (b) future
//! zone-level dominant-biome queries.
//!
//! Same lazy-`OnceLock` registry pattern as `definition_core` /
//! `recipe_core`: first access builds, errors are sticky.

use std::sync::OnceLock;

use serde_json::Value;

use crate::definition_core::find_packed_by_key;

pub type BiomeId = u8;

/// Sentinel "no biome" id. Biome IDs are 1-indexed; `BIOME_NONE`
/// returned when no biome envelope contains the cell's climate.
pub const BIOME_NONE: BiomeId = 0;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiomeDefinition {
    pub id: BiomeId,
    /// Key from the JSON, e.g. `"forest"`. Used for dev-side
    /// logging / debug overlays; not stable wire data.
    pub key: String,
    /// `[elevation_min, elevation_max]`. Inclusive on both ends. A
    /// cell's elevation sample lands in the biome's range when
    /// `min <= sample <= max`. Omitted in JSON ⇒ `(0.0, 1.0)`.
    pub elevation_range: (f32, f32),
    pub temperature_range: (f32, f32),
    pub humidity_range: (f32, f32),
    pub aether_range: (f32, f32),
    /// Packed `[card_type:u4 | def_id:u12]` of the biome's barren
    /// base tile — what `world_gen::biome_for` returns for the
    /// revert-on-consume path. `None` for biomes without an
    /// authored base tile (e.g. an ocean biome before water tiles
    /// land); callers fall back to "leave the cell empty" or sample
    /// the climate envelope themselves.
    pub base_tile_packed: Option<u16>,
}

struct BiomeRegistry {
    by_id: Vec<BiomeDefinition>,
}

const BIOMES_JSON: &str = include_str!("../zones/biomes.json");
static BIOMES: OnceLock<Result<BiomeRegistry, String>> = OnceLock::new();

fn biomes_registry() -> Result<&'static BiomeRegistry, String> {
    BIOMES.get_or_init(build_biomes).as_ref().map_err(|e| e.clone())
}

/// Walk the biome list in declaration order; return the id of the
/// first biome whose four-axis envelope contains the cell's climate
/// samples. Returns `BIOME_NONE` when no biome matches — callers
/// decide whether to fall back to the last-declared biome or treat
/// the cell as biomeless.
///
/// `Err` only on registry-build failure (malformed `biomes.json` or
/// an unresolvable `base_tile` key).
pub fn biome_for_climate(
    elevation: f32,
    temperature: f32,
    humidity: f32,
    aether: f32,
) -> Result<BiomeId, String> {
    let registry = biomes_registry()?;
    for b in &registry.by_id {
        if elevation >= b.elevation_range.0
            && elevation <= b.elevation_range.1
            && temperature >= b.temperature_range.0
            && temperature <= b.temperature_range.1
            && humidity >= b.humidity_range.0
            && humidity <= b.humidity_range.1
            && aether >= b.aether_range.0
            && aether <= b.aether_range.1
        {
            return Ok(b.id);
        }
    }
    Ok(BIOME_NONE)
}

/// Resolve a `BiomeId` to its definition. `Ok(None)` for `BIOME_NONE`
/// or out-of-range ids; `Err` on registry-build failure.
pub fn biome(id: BiomeId) -> Result<Option<&'static BiomeDefinition>, String> {
    if id == BIOME_NONE {
        return Ok(None);
    }
    Ok(biomes_registry()?.by_id.get((id - 1) as usize))
}

/// All biomes in declaration order. Same order the climate-walk uses,
/// so callers can inspect what `biome_for_climate` would do without
/// duplicating the walk.
pub fn biomes() -> Result<&'static [BiomeDefinition], String> {
    Ok(&biomes_registry()?.by_id)
}

fn build_biomes() -> Result<BiomeRegistry, String> {
    let root: Value = serde_json::from_str(BIOMES_JSON)
        .map_err(|e| format!("biomes.json: parse failed: {}", e))?;
    let root = root
        .as_object()
        .ok_or_else(|| "biomes.json: top-level not an object".to_string())?;
    let biomes_obj = root
        .get("biomes")
        .and_then(Value::as_object)
        .ok_or_else(|| "biomes.json: missing 'biomes' object".to_string())?;

    let mut by_id: Vec<BiomeDefinition> = Vec::with_capacity(biomes_obj.len());
    let mut next_id: u32 = 1;

    for (key, value) in biomes_obj {
        if key.starts_with('_') {
            continue;
        }
        let obj = value.as_object().ok_or_else(|| {
            format!("biomes.json: biome {:?} not an object", key)
        })?;

        if next_id > BiomeId::MAX as u32 {
            return Err(format!("biomes.json: more than {} biomes", BiomeId::MAX));
        }

        let elevation_range = parse_range(obj, key, "elevation")?;
        let temperature_range = parse_range(obj, key, "temperature")?;
        let humidity_range = parse_range(obj, key, "humidity")?;
        let aether_range = parse_range(obj, key, "aether")?;

        // `base_tile` is optional — a biome without one means
        // `world_gen::biome_for` would return None for cells in this
        // envelope. v1 every biome declares one; left optional so
        // future biomes (ocean, void) can ship before their tiles
        // exist.
        let base_tile_packed = match obj.get("base_tile") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(
                find_packed_by_key(s)?.ok_or_else(|| {
                    format!(
                        "biomes.json: biome {:?}.base_tile {:?} not found in card registry",
                        key, s
                    )
                })?,
            ),
            Some(_) => {
                return Err(format!(
                    "biomes.json: biome {:?}.base_tile not a string",
                    key
                ));
            }
        };

        by_id.push(BiomeDefinition {
            id: next_id as BiomeId,
            key: key.clone(),
            elevation_range,
            temperature_range,
            humidity_range,
            aether_range,
            base_tile_packed,
        });
        next_id += 1;
    }

    Ok(BiomeRegistry { by_id })
}

/// Parse a `[min, max]` array out of a biome's JSON object. Omitted
/// field ⇒ `(0.0, 1.0)` ("no constraint on this axis"). Validates
/// the array is exactly two numbers, both in `[0, 1]`, and
/// `min <= max`.
fn parse_range(
    obj: &serde_json::Map<String, Value>,
    biome_key: &str,
    field: &str,
) -> Result<(f32, f32), String> {
    match obj.get(field) {
        None => Ok((0.0, 1.0)),
        Some(Value::Array(arr)) => {
            if arr.len() != 2 {
                return Err(format!(
                    "biomes.json: biome {:?}.{} must be a 2-element array",
                    biome_key, field
                ));
            }
            let lo = arr[0].as_f64().ok_or_else(|| {
                format!(
                    "biomes.json: biome {:?}.{}[0] not a number",
                    biome_key, field
                )
            })? as f32;
            let hi = arr[1].as_f64().ok_or_else(|| {
                format!(
                    "biomes.json: biome {:?}.{}[1] not a number",
                    biome_key, field
                )
            })? as f32;
            if !(0.0..=1.0).contains(&lo) || !(0.0..=1.0).contains(&hi) || lo > hi {
                return Err(format!(
                    "biomes.json: biome {:?}.{} = [{}, {}] out of range [0, 1] or lo > hi",
                    biome_key, field, lo, hi
                ));
            }
            Ok((lo, hi))
        }
        Some(_) => Err(format!(
            "biomes.json: biome {:?}.{} not an array",
            biome_key, field
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biomes_registry_builds() {
        biomes_registry().expect("biomes registry should build");
    }

    #[test]
    fn biome_for_climate_returns_none_outside_any_envelope() {
        // Climate values at the extreme corners shouldn't match any
        // biome (elevation 0 is below every biome's lower bound).
        let id = biome_for_climate(0.0, 0.0, 0.0, 0.0).unwrap();
        assert_eq!(id, BIOME_NONE);
    }

    #[test]
    fn biome_for_climate_walks_in_order() {
        // High-elevation cell should land in mountain regardless of
        // other axes — mountain is declared first and only
        // constrains elevation.
        let id = biome_for_climate(0.85, 0.5, 0.5, 0.5).unwrap();
        let b = biome(id).unwrap().expect("biome record");
        assert_eq!(b.key, "mountain");
    }
}
