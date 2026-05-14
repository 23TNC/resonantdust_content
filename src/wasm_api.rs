//! JS-facing wasm-bindgen API. Only compiled when the `js` feature is
//! enabled (gated in `lib.rs`). Each export wraps a function from
//! `definition_core` (and later `recipe_core`) and converts the result
//! into a JS-friendly form: errors become thrown JS strings, missing rows
//! become `null` or `undefined`, and `&'static CardDefinition` references
//! are serialized into plain JS objects via `serde_wasm_bindgen`.
//!
//! Field names on serialized structs are renamed to camelCase via
//! `#[serde(rename_all = "camelCase")]` on the source structs, so JS-side
//! consumers see `cardType` / `cardCategory` / `definitionId` rather than
//! the Rust snake_case names.

use wasm_bindgen::prelude::*;

use crate::definition_core::{
  decode_definition as core_decode_definition,
  find_packed_by_key as core_find_packed_by_key,
  is_hex_type as core_is_hex_type,
  CardDefinition,
};
use crate::flags_core::{
  card_flag_bit as core_card_flag_bit, card_flag_field as core_card_flag_field,
};
use crate::recipe_core::{
  match_stack_recipe_detail as core_match_stack_recipe_detail,
  HasCandidates, StackDirection,
};

/// Decode a packed `(cardType:u4 | cardCategory:u4 | definitionId:u8)` value
/// into a `CardDefinition`-shaped JS object. Returns `null` if no card
/// matches the packed value. Throws a string error if the card registry
/// failed to build (malformed JSON, unknown aspects, etc.).
#[wasm_bindgen(js_name = decodeDefinition)]
pub fn decode_definition(packed: u16) -> Result<JsValue, JsValue> {
  let opt = core_decode_definition(packed).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(def) => serde_wasm_bindgen::to_value(def)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// Look up a card's packed value by its bare key (e.g. `"fatigue"`).
/// Returns `undefined` if no card has that key. Throws on registry-build
/// failure.
#[wasm_bindgen(js_name = findPackedByKey)]
pub fn find_packed_by_key(key: &str) -> Result<Option<u16>, JsValue> {
  core_find_packed_by_key(key).map_err(|e| JsValue::from_str(&e))
}

/// Whether the given `cardType` id resolves to a hex-shaped type
/// (`"hex"` in `cards/types.json`). Throws on registry-build failure.
#[wasm_bindgen(js_name = isHexType)]
pub fn is_hex_type(type_id: u8) -> Result<bool, JsValue> {
  core_is_hex_type(type_id).map_err(|e| JsValue::from_str(&e))
}

/// Bit position (0..=7) of a card-flag by name (e.g. `"drop_hold"`,
/// `"position_locked"`, `"dead"`). Returns `undefined` if no flag with
/// that name is declared in `cards/flags.json`. Throws on registry-build
/// failure. JS-side callers typically convert to a mask via
/// `1 << bit` before testing against `row.flags`.
#[wasm_bindgen(js_name = cardFlagBit)]
pub fn card_flag_bit(name: &str) -> Result<Option<u8>, JsValue> {
  core_card_flag_bit(name).map_err(|e| JsValue::from_str(&e))
}

/// Read the value of a multi-bit card-flag field (e.g.
/// `"progress_style"`, `"position_hold_count"`) out of a `flags`
/// u32. Returns `undefined` if no field with that name is declared in
/// `cards/flags.json`; returns the extracted unsigned value
/// otherwise. Throws on registry-build failure.
///
/// Equivalent to `(flags >> field.shift) & field.mask`. JS-side
/// callers checking "is the count > 0?" use `value > 0`; callers
/// reading specific enum-style values (`progress_style == 1`)
/// compare directly.
#[wasm_bindgen(js_name = cardFlagFieldValue)]
pub fn card_flag_field_value(flags: u32, name: &str) -> Result<Option<u32>, JsValue> {
  let field = core_card_flag_field(name).map_err(|e| JsValue::from_str(&e))?;
  Ok(field.map(|f| (flags & f.mask()) >> f.shift as u32))
}

/// Find the best-matching `Stack(direction)` recipe for a chain.
/// `hex_def` is the packed definition of the hex card the chain root is
/// attached to (`0` if not stacked on hex). `root_def` is the loose
/// root's packed definition. `slot_defs` are the packed definitions of
/// cards stacked above (`direction = 0` / "up") or below
/// (`direction = 1` / "down") the root, in chain order.
///
/// `root_above` / `actor_above` / `root_below` / `actor_below` are the
/// packed definitions of cards stacked on each role's soul card in
/// each direction (UP = equipment / above the soul, DOWN = action
/// stack / below the soul). They feed the `has` / `reagents.has` /
/// `has_below` / `reagents.has_below` feasibility filter: recipes
/// whose has-predicates can't find any matching card in the
/// corresponding pool are skipped before scoring. Pass empty arrays
/// to mean "no equipment / nothing on the soul stack" — recipes
/// that declare has-predicates will then be filtered out, which is
/// the correct behaviour for an unattached player.
///
/// Unknown packed defs in any pool array are silently skipped (treat
/// the registry as authoritative — a wire-side glitch shouldn't
/// crash matching).
///
/// Returns a `StackMatch` object on success (with `recipeIndex`,
/// `slotStart`, `slotCount`, `hasRoot`, `hasHex`) or `null` if no
/// recipe matched. Throws on registry-build failure or invalid
/// direction.
#[wasm_bindgen(js_name = matchStackRecipe)]
pub fn match_stack_recipe(
  hex_def: u16,
  root_def: u16,
  slot_defs: Vec<u16>,
  direction: u8,
  root_above: Vec<u16>,
  actor_above: Vec<u16>,
  root_below: Vec<u16>,
  actor_below: Vec<u16>,
) -> Result<JsValue, JsValue> {
  let dir = match direction {
    0 => StackDirection::Up,
    1 => StackDirection::Down,
    _ => {
      return Err(JsValue::from_str(&format!(
        "matchStackRecipe: invalid direction {} (expected 0 = up, 1 = down)",
        direction,
      )));
    }
  };
  let root_above_defs = decode_def_pool(&root_above)?;
  let actor_above_defs = decode_def_pool(&actor_above)?;
  let root_below_defs = decode_def_pool(&root_below)?;
  let actor_below_defs = decode_def_pool(&actor_below)?;
  let has_candidates = HasCandidates {
    root_above: root_above_defs,
    actor_above: actor_above_defs,
    root_below: root_below_defs,
    actor_below: actor_below_defs,
  };
  let opt = core_match_stack_recipe_detail(
    hex_def,
    root_def,
    &slot_defs,
    dir,
    Some(&has_candidates),
  )
  .map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(m) => serde_wasm_bindgen::to_value(&m)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// Decode a packed-def array into `&CardDefinition` refs, dropping
/// any entries that don't resolve. Used by `match_stack_recipe` to
/// build the four has-candidate pools — unknown packed values are
/// treated as "card not in catalog" rather than fatal, so a stale
/// client def won't crash matching while a registry rebuild is
/// pending.
fn decode_def_pool(packed: &[u16]) -> Result<Vec<&'static CardDefinition>, JsValue> {
  let mut out = Vec::with_capacity(packed.len());
  for &p in packed {
    match core_decode_definition(p).map_err(|e| JsValue::from_str(&e))? {
      Some(def) => out.push(def),
      None => continue,
    }
  }
  Ok(out)
}
