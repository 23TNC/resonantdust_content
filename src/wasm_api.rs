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
