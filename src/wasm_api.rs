//! JS-facing wasm-bindgen API. Only compiled when the `js` feature is
//! enabled (gated in `lib.rs`). Each export wraps a function from
//! `definition_core` (and later `recipe_core`) and converts the result
//! into a JS-friendly form: errors become thrown JS strings, missing rows
//! become `null` or `undefined`, and `&'static CardDefinition` references
//! are serialized into plain JS objects via `serde_wasm_bindgen`.
//!
//! Field names on serialized structs are renamed to camelCase via
//! `#[serde(rename_all = "camelCase")]` on the source structs, so JS-side
//! consumers see `cardType` / `definitionId` rather than the Rust
//! snake_case names.

use wasm_bindgen::prelude::*;

use crate::definition_core::{
  aspect as core_aspect,
  aspect_id as core_aspect_id,
  card_locale_path as core_card_locale_path,
  decode_definition as core_decode_definition,
  find_packed_by_key as core_find_packed_by_key,
  is_hex_type as core_is_hex_type,
};
use crate::flags_core::{
  card_flag_bit as core_card_flag_bit,
  card_flag_field as core_card_flag_field,
  flag_bit as core_flag_bit,
  flag_field as core_flag_field,
};
use crate::recipe_core::{
  find_recipe as core_find_recipe,
  recipe as core_recipe,
  recipes_by_priority as core_recipes_by_priority,
};
use crate::starter_pack_core::{
  starter_blueprints_for_soul as core_starter_blueprints_for_soul,
  starter_packs_for_soul as core_starter_packs_for_soul,
};
use crate::blueprint_core::{
  blueprint as core_blueprint,
  blueprints_all as core_blueprints_all,
  find_blueprint as core_find_blueprint,
  BlueprintScope,
};
use crate::texture_core::textures as core_textures;

/// Look up an aspect by id. Returns the `Aspect` object (with `id`,
/// `name`, `description`, `icon`, `group` fields) or `null` for
/// `ASPECT_NONE` (id 0) and unknown ids. Throws on registry-build
/// failure.
#[wasm_bindgen(js_name = aspectInfo)]
pub fn aspect_info(id: u8) -> Result<JsValue, JsValue> {
  let opt = core_aspect(id).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(a) => serde_wasm_bindgen::to_value(a)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// Look up an aspect's numeric id by its declared name (the JSON
/// key under `cards/aspects.json` — `"wood"`, `"corpus+"`, etc.).
/// Returns `undefined` when the name isn't registered. Throws on
/// registry-build failure.
///
/// Used by the client recipe matcher to evaluate
/// `<path>.aspect.<name>.min: <N>` predicates: the name appears in
/// the recipe segments, but card defs store aspect entries keyed by
/// numeric id — this helper bridges the two.
#[wasm_bindgen(js_name = aspectIdByName)]
pub fn aspect_id_by_name(name: &str) -> Result<Option<u8>, JsValue> {
  core_aspect_id(name).map_err(|e| JsValue::from_str(&e))
}

/// Decode a packed `(cardType:u4 | definitionId:u12)` value into a
/// `CardDefinition`-shaped JS object. Returns `null` if no card
/// matches the packed value. Throws a string error if the card
/// registry failed to build (malformed JSON, unknown aspects, etc.).
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

/// Look up the display label for a packed definition in the given
/// language, e.g. `cardLabel(packed, "en")` → `"Log"`. Falls back to
/// English when `lang` has no entry. Returns `undefined` for unknown
/// packed ids or locale entries with no label. Throws on registry-build
/// failure. Callers should fall back to `def.key` on `undefined`.
#[wasm_bindgen(js_name = cardLabel)]
pub fn card_label(packed_def: u16, lang: &str) -> Result<Option<String>, JsValue> {
  let path = core_card_locale_path(packed_def).map_err(|e| JsValue::from_str(&e))?;
  let Some(path) = path else { return Ok(None) };
  let label = crate::locales_core::label("cards", lang, &path)
    .map_err(|e| JsValue::from_str(&e))?;
  Ok(label.map(String::from))
}

/// **Legacy** — bit position (0..=31) of a card-flag by name, searched
/// across both `cards_state` and `cards_bk` fields (state first).
/// Returns `undefined` if no single-bit flag with that name exists in
/// either field. Ambiguous against the split-field schema — callers
/// that need to know which host integer the bit lives in should use
/// [`cardFlagBitIn`] with an explicit field name instead.
#[wasm_bindgen(js_name = cardFlagBit)]
pub fn card_flag_bit(name: &str) -> Result<Option<u8>, JsValue> {
  core_card_flag_bit(name).map_err(|e| JsValue::from_str(&e))
}

/// Bit position (0..=31) of a single-bit flag in a specific field.
/// `field` is `"cards_state"` or `"cards_bk"`. Returns `undefined` if
/// no single-bit flag with that name is declared in the given field.
/// Preferred over [`cardFlagBit`] for new call sites — explicit field
/// argument means lookups can't accidentally collide across fields.
#[wasm_bindgen(js_name = cardFlagBitIn)]
pub fn card_flag_bit_in(field: &str, name: &str) -> Result<Option<u8>, JsValue> {
  core_flag_bit(field, name).map_err(|e| JsValue::from_str(&e))
}

/// `(shift, width)` of a multi-bit flag field in a specific field.
/// Returns `undefined` if no multi-bit field with that name is
/// declared in the given field. Use the returned pair to mask:
/// `mask = ((1 << width) - 1) << shift`, value extract:
/// `(host >> shift) & ((1 << width) - 1)`.
#[wasm_bindgen(js_name = cardFlagFieldShape)]
pub fn card_flag_field_shape(field: &str, name: &str) -> Result<Option<Vec<u8>>, JsValue> {
  let f = core_flag_field(field, name).map_err(|e| JsValue::from_str(&e))?;
  Ok(f.map(|f| vec![f.shift, f.width]))
}

/// Look up a `card_type` id by name (e.g. `"mini_zone"`, `"soul"`,
/// `"tile"`). Returns `undefined` for unknown names. Source of truth
/// is `content/cards/types.json`. Used by JS-side code that needs to
/// branch on a card's type (without hard-coding the numeric id).
#[wasm_bindgen(js_name = cardTypeId)]
pub fn card_type_id(name: &str) -> Result<Option<u8>, JsValue> {
  let ids = crate::definition_core::card_type_ids().map_err(|e| JsValue::from_str(&e))?;
  Ok(ids.get(name).copied())
}

/// **Legacy** — read the value of a multi-bit card-flag field by
/// name, searching across both `cards_state` and `cards_bk` (state
/// first). Caller passes a single `flags` u32 that should be the
/// matching host integer; ambiguous against the split-field schema.
/// Prefer [`cardFlagFieldValueIn`] with an explicit field name for
/// new call sites.
#[wasm_bindgen(js_name = cardFlagFieldValue)]
pub fn card_flag_field_value(flags: u32, name: &str) -> Result<Option<u32>, JsValue> {
  let field = core_card_flag_field(name).map_err(|e| JsValue::from_str(&e))?;
  Ok(field.map(|f| (flags & f.mask()) >> f.shift as u32))
}

/// Read the value of a multi-bit field in a specific host integer.
/// `field` is `"cards_state"` or `"cards_bk"`; `host` is the value
/// of the corresponding `Card.flags_state` / `Card.flags_bk` column.
/// Returns `undefined` if no multi-bit field with that name is
/// declared in the given field.
#[wasm_bindgen(js_name = cardFlagFieldValueIn)]
pub fn card_flag_field_value_in(
  field: &str,
  host: u32,
  name: &str,
) -> Result<Option<u32>, JsValue> {
  let f = core_flag_field(field, name).map_err(|e| JsValue::from_str(&e))?;
  Ok(f.map(|f| (host & f.mask()) >> f.shift as u32))
}

/// Field-routing helper — given **both** flag host integers and a
/// flag name, returns `true` if the named single-bit flag is set in
/// whichever field declares it. Looks up `cards_state` first then
/// `cards_bk`; consults only the matching host. Callers pass the
/// whole `(state, bk)` pair from the card row so the lookup is
/// unambiguous against the split schema.
///
/// Returns `false` for unknown flag names (the safe default for
/// "absent") and for cards whose bit is clear in the matching host.
#[wasm_bindgen(js_name = hasCardFlag)]
pub fn has_card_flag(state: u32, bk: u32, name: &str) -> Result<bool, JsValue> {
  if let Some(bit) = core_flag_bit("cards_state", name).map_err(|e| JsValue::from_str(&e))? {
    return Ok(state & (1u32 << bit) != 0);
  }
  if let Some(bit) = core_flag_bit("cards_bk", name).map_err(|e| JsValue::from_str(&e))? {
    return Ok(bk & (1u32 << bit) != 0);
  }
  Ok(false)
}

/// Field-routing helper for multi-bit fields — given both host
/// integers and a field name, returns the extracted value from
/// whichever field declares it (state-first lookup). Returns
/// `undefined` for unknown field names.
#[wasm_bindgen(js_name = cardFlagFieldValueAny)]
pub fn card_flag_field_value_any(state: u32, bk: u32, name: &str) -> Result<Option<u32>, JsValue> {
  if let Some(f) = core_flag_field("cards_state", name).map_err(|e| JsValue::from_str(&e))? {
    return Ok(Some((state & f.mask()) >> f.shift as u32));
  }
  if let Some(f) = core_flag_field("cards_bk", name).map_err(|e| JsValue::from_str(&e))? {
    return Ok(Some((bk & f.mask()) >> f.shift as u32));
  }
  Ok(None)
}

/// Read the numeric value of a named aspect off a packed card
/// definition. Returns `null` when:
/// - the aspect name isn't in `aspects.json`,
/// - the def doesn't carry that aspect,
/// - or the packed def doesn't resolve to a registered card.
///
/// Source-of-truth pair with the server's
/// `def.aspect_value(aspect_id("name"))` path — both go through the
/// same `CardDefinition::aspect_value` lookup, so client and server
/// agree on cost / speed / inventory / etc. numbers by construction.
///
/// Used by client A* (`pixijs/src/game/world/pathfind.ts`) to
/// resolve per-tile `cost` and per-soul `speed` for the step-time
/// calculation, mirroring the server validator in
/// `movement::move_soul_path`.
#[wasm_bindgen(js_name = aspectValue)]
pub fn aspect_value(packed_def: u16, name: &str) -> Result<Option<f32>, JsValue> {
  let aid = match crate::definition_core::aspect_id(name)
    .map_err(|e| JsValue::from_str(&e))?
  {
    Some(id) => id,
    None => return Ok(None),
  };
  let def = match crate::definition_core::decode_definition(packed_def)
    .map_err(|e| JsValue::from_str(&e))?
  {
    Some(d) => d,
    None => return Ok(None),
  };
  Ok(def.aspect_value(aid))
}

/// Look up a recipe by its stable `u16` id (the value
/// `proposeAction` takes as `recipeId`). Returns the full Recipe IR
/// serialized to JS — `{ id, input[], output[], iterators[], anchors }`
/// — or `null` if the id isn't registered. Throws on registry-build
/// failure.
///
/// Used by callers that already have the id (e.g., looking up a
/// magnetic recipe via a card def's `magnetic.recipe` key resolved
/// through `findPackedByKey`-style indirection).
#[wasm_bindgen(js_name = recipeById)]
pub fn recipe_by_id(id: u16) -> Result<JsValue, JsValue> {
  let opt = core_recipe(id).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(r) => serde_wasm_bindgen::to_value(r)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// Look up a recipe by its source-key (e.g. `"cut_tree"`,
/// `"strike_success"`). Returns the full Recipe IR serialized to JS
/// or `null` if no recipe with that key is registered. Throws on
/// registry-build failure.
///
/// Used by callers that have a string key in hand — for example, a
/// card def's `magnetic.recipe` field, or recipe-name lookups in
/// debug tooling.
#[wasm_bindgen(js_name = recipeByKey)]
pub fn recipe_by_key(key: &str) -> Result<JsValue, JsValue> {
  let opt = core_find_recipe(key).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(r) => serde_wasm_bindgen::to_value(r)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// JS-facing entry in the `recipesAll` response: stable id plus the
/// full Recipe IR. The id is what `proposeAction` carries on the
/// wire; the recipe carries every field the client matcher needs to
/// walk iterators and evaluate predicates.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RecipeWithId<'a> {
  id: u16,
  recipe: &'a crate::recipe_core::Recipe,
}

/// Every registered recipe in priority-tiered order (highest priority
/// first). Each entry is `{ id: u16, recipe: Recipe }` — the `id` is
/// the stable u16 from `recipes/id.json` (what `proposeAction` takes),
/// the `recipe` is the parsed IR including `iterators` and `anchors`.
///
/// Priority order is determined by [`crate::recipe_core::AnchorSet`]
/// — anchor count first, then anchor priority (hex > root > up > down).
/// The client matcher walks this array in order and stops at the first
/// tier that yields successful binding(s).
///
/// Returns an empty array when no recipes are registered. Throws on
/// registry-build failure.
#[wasm_bindgen(js_name = recipesAll)]
pub fn recipes_all() -> Result<JsValue, JsValue> {
  let by_priority = core_recipes_by_priority().map_err(|e| JsValue::from_str(&e))?;
  // The `recipe_core` API gives us `&Recipe`s but no parallel id
  // list — so walk each one's source-key back to its id via
  // `find_recipe_id`. Cheap (BTreeMap lookup per recipe), runs once
  // at client startup.
  let mut out: Vec<RecipeWithId> = Vec::with_capacity(by_priority.len());
  for r in &by_priority {
    let id = crate::recipe_core::find_recipe_id(&r.id)
      .map_err(|e| JsValue::from_str(&e))?
      .ok_or_else(|| {
        JsValue::from_str(&format!(
          "recipesAll: recipe {:?} present in registry but missing from id map (registry corrupt)",
          r.id
        ))
      })?;
    out.push(RecipeWithId { id, recipe: r });
  }
  serde_wasm_bindgen::to_value(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// All starter packs registered for a given soul card key (e.g.
/// `"human"`). Returns an array of `StarterPack` objects (`id`,
/// `soul`, `packId`, `contents: [{cardKey, packedDefinition,
/// count}]`). Empty array for unknown soul keys. Throws on
/// registry-build failure.
///
/// Used by the character-create panel to enumerate which packs the
/// player can pick from. JS-side filtering by soul is unnecessary
/// since this is already soul-scoped at the call site.
#[wasm_bindgen(js_name = starterPacksForSoul)]
pub fn starter_packs_for_soul(soul: &str) -> Result<JsValue, JsValue> {
  let packs = core_starter_packs_for_soul(soul).map_err(|e| JsValue::from_str(&e))?;
  serde_wasm_bindgen::to_value(&packs).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Stable blueprint ids granted to a player on creating a character
/// of the given soul. Sourced from the soul's `"blueprints"` array in
/// `starter_packs/data/*.json`. Returns an empty array for souls that
/// don't declare any. Throws on registry-build failure.
///
/// Each id resolves to a full `Blueprint` via `blueprintById`.
#[wasm_bindgen(js_name = starterBlueprintsForSoul)]
pub fn starter_blueprints_for_soul(soul: &str) -> Result<Vec<u16>, JsValue> {
  core_starter_blueprints_for_soul(soul).map_err(|e| JsValue::from_str(&e))
}

/// Look up a soul-scope blueprint by its stable `u16` id. Returns
/// the full Blueprint object or `null` if the id isn't registered.
/// Throws on registry-build failure.
#[wasm_bindgen(js_name = blueprintById)]
pub fn blueprint_by_id(id: u16) -> Result<JsValue, JsValue> {
  let opt = core_blueprint(BlueprintScope::Soul, id).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(bp) => serde_wasm_bindgen::to_value(bp)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// Look up a soul-scope blueprint by its source-key. Returns the
/// full Blueprint object or `null`. Throws on registry-build failure.
#[wasm_bindgen(js_name = blueprintByKey)]
pub fn blueprint_by_key(key: &str) -> Result<JsValue, JsValue> {
  let opt = core_find_blueprint(BlueprintScope::Soul, key).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(bp) => serde_wasm_bindgen::to_value(bp)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// Every registered soul-scope blueprint in stable-id order.
/// Called by the wrench panel to enumerate the catalog for display.
#[wasm_bindgen(js_name = allBlueprints)]
pub fn all_blueprints() -> Result<JsValue, JsValue> {
  let bps = core_blueprints_all(BlueprintScope::Soul).map_err(|e| JsValue::from_str(&e))?;
  serde_wasm_bindgen::to_value(&bps).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Player-scope analog of [`blueprint_by_id`].
#[wasm_bindgen(js_name = playerBlueprintById)]
pub fn player_blueprint_by_id(id: u16) -> Result<JsValue, JsValue> {
  let opt = core_blueprint(BlueprintScope::Player, id).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(bp) => serde_wasm_bindgen::to_value(bp)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// Player-scope analog of [`blueprint_by_key`].
#[wasm_bindgen(js_name = playerBlueprintByKey)]
pub fn player_blueprint_by_key(key: &str) -> Result<JsValue, JsValue> {
  let opt = core_find_blueprint(BlueprintScope::Player, key).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(bp) => serde_wasm_bindgen::to_value(bp)
      .map_err(|e| JsValue::from_str(&e.to_string())),
    None => Ok(JsValue::NULL),
  }
}

/// Player-scope analog of [`all_blueprints`]. Called by the dna
/// (🧬) panel to enumerate the player-blueprint catalog.
#[wasm_bindgen(js_name = allPlayerBlueprints)]
pub fn all_player_blueprints() -> Result<JsValue, JsValue> {
  let bps = core_blueprints_all(BlueprintScope::Player).map_err(|e| JsValue::from_str(&e))?;
  serde_wasm_bindgen::to_value(&bps).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Every registered texture definition, in stable-id order. Each entry
/// carries `id`, `aspectId`, `aspectName`, `size`,
/// `scale: { min, max }`, and `anchor: { x, y }`. Returns an empty
/// array when no aspect carries render metadata. Throws on
/// registry-build failure.
///
/// Post card-object unification (see
/// docs/CARD_OBJECT_UNIFICATION.md) entries are aspect-keyed and the
/// pack-folder on disk is named `<size>_<aspectName>_pack/` — pack
/// name and aspect name are the same string.
///
/// Called once at startup by `TextureRegistry.ts` to build the
/// client-side lookup map; not intended for per-frame use.
#[wasm_bindgen(js_name = allTextures)]
pub fn all_textures() -> Result<JsValue, JsValue> {
  let txs = core_textures().map_err(|e| JsValue::from_str(&e))?;
  serde_wasm_bindgen::to_value(&txs).map_err(|e| JsValue::from_str(&e.to_string()))
}

// ---- Bit-packing helpers ------------------------------------------------
//
// Same definitions the server uses — both shard and chat re-export
// `resonantdust_content::packed` so changes to the bit layouts here
// propagate to the whole stack. The client today has native TS
// implementations in `pixijs/src/server/data/packing.ts` for hot-path
// reasons (called per zone decode / card sync); the wasm exports
// below let cold paths or test fixtures call into the canonical
// implementation when wasm-crossing overhead doesn't matter.
//
// Result struct shapes are flat objects (`{ q, r }`, `{ cardType,
// defId }`, etc.) keyed by camelCase fields, matching the rest of
// the wasm API's serialisation discipline. Numbers use plain `u32`
// where the bit layout fits; `u64` (valid_at) maps to JS BigInt
// automatically via wasm-bindgen.

use crate::packed as core_packed;

#[wasm_bindgen(js_name = packValidAt)]
pub fn pack_valid_at(time_ms: u64, sequence: u16) -> u64 {
  core_packed::pack_valid_at(time_ms, sequence)
}

#[wasm_bindgen(js_name = validAtTime)]
pub fn valid_at_time(packed: u64) -> u64 {
  core_packed::valid_at_time(packed)
}

#[wasm_bindgen(js_name = packMacroZone)]
pub fn pack_macro_zone(q: i16, r: i16) -> u32 {
  core_packed::pack_macro_zone(q, r)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MacroZoneUnpacked {
  q: i16,
  r: i16,
}

#[wasm_bindgen(js_name = unpackMacroZone)]
pub fn unpack_macro_zone(v: u32) -> Result<JsValue, JsValue> {
  let (q, r) = core_packed::unpack_macro_zone(v);
  serde_wasm_bindgen::to_value(&MacroZoneUnpacked { q, r })
    .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = packMicroZone)]
pub fn pack_micro_zone(q: u8, r: u8, stacked_state: u8) -> u8 {
  core_packed::pack_micro_zone(q, r, core_packed::StackedState::from_u2(stacked_state))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MicroZoneUnpacked {
  q: u8,
  r: u8,
  stacked_state: u8,
}

#[wasm_bindgen(js_name = unpackMicroZone)]
pub fn unpack_micro_zone(v: u8) -> Result<JsValue, JsValue> {
  let (q, r, state) = core_packed::unpack_micro_zone(v);
  serde_wasm_bindgen::to_value(&MicroZoneUnpacked {
    q,
    r,
    stacked_state: state.to_u2(),
  })
  .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = packStackMicroZone)]
pub fn pack_stack_micro_zone(position: u8, direction: u8, stacked_state: u8) -> u8 {
  core_packed::pack_stack_micro_zone(
    position,
    direction,
    core_packed::StackedState::from_u2(stacked_state),
  )
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StackMicroZoneUnpacked {
  position: u8,
  direction: u8,
  stacked_state: u8,
}

#[wasm_bindgen(js_name = unpackStackMicroZone)]
pub fn unpack_stack_micro_zone(v: u8) -> Result<JsValue, JsValue> {
  let (position, direction, state) = core_packed::unpack_stack_micro_zone(v);
  serde_wasm_bindgen::to_value(&StackMicroZoneUnpacked {
    position,
    direction,
    stacked_state: state.to_u2(),
  })
  .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = packSlotMicroZone)]
pub fn pack_slot_micro_zone(direction: u8) -> u8 {
  core_packed::pack_slot_micro_zone(direction)
}

#[wasm_bindgen(js_name = isStackLayout)]
pub fn is_stack_layout(stacked_state: u8, surface: u8) -> bool {
  core_packed::is_stack_layout(
    core_packed::StackedState::from_u2(stacked_state),
    surface,
  )
}

#[wasm_bindgen(js_name = packDefinition)]
pub fn pack_definition(card_type: u8, def_id: u16) -> u16 {
  core_packed::pack_definition(card_type, def_id)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DefinitionUnpacked {
  card_type: u8,
  def_id: u16,
}

#[wasm_bindgen(js_name = unpackDefinition)]
pub fn unpack_definition(v: u16) -> Result<JsValue, JsValue> {
  let (card_type, def_id) = core_packed::unpack_definition(v);
  serde_wasm_bindgen::to_value(&DefinitionUnpacked { card_type, def_id })
    .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = packZoneDefinition)]
pub fn pack_zone_definition(card_type: u8) -> u8 {
  core_packed::pack_zone_definition(card_type)
}

#[wasm_bindgen(js_name = unpackZoneDefinition)]
pub fn unpack_zone_definition(v: u8) -> u8 {
  core_packed::unpack_zone_definition(v)
}

// Surface band constants exposed as `#[wasm_bindgen]` getters so JS
// reads `WORLD_LAYER()` etc. — TS-side already exports identical
// values from `packing.ts` for hot-path use; these wasm getters are
// for callers that prefer the single-source-of-truth path.

#[wasm_bindgen(js_name = inventoryLayer)]
pub fn inventory_layer() -> u8 {
  core_packed::INVENTORY_LAYER
}

#[wasm_bindgen(js_name = pocketDimensionLayer)]
pub fn pocket_dimension_layer() -> u8 {
  core_packed::POCKET_DIMENSION_LAYER
}

#[wasm_bindgen(js_name = miniZoneLayer)]
pub fn mini_zone_layer() -> u8 {
  core_packed::MINI_ZONE_LAYER
}

#[wasm_bindgen(js_name = worldLayer)]
pub fn world_layer() -> u8 {
  core_packed::WORLD_LAYER
}
