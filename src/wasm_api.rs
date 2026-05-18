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
  card_locale_path as core_card_locale_path,
  decode_definition as core_decode_definition,
  find_packed_by_key as core_find_packed_by_key,
  is_hex_type as core_is_hex_type,
  CardDefinition,
};
use crate::flags_core::{
  card_flag_bit as core_card_flag_bit, card_flag_field as core_card_flag_field,
};
use crate::recipe_core::{
  find_recipe as core_find_recipe,
  match_magnetic_recipe as core_match_magnetic_recipe,
  match_stack_recipe_detail as core_match_stack_recipe_detail,
  HasCandidates, StackDirection,
};
use crate::starter_pack_core::{
  starter_packs_for_soul as core_starter_packs_for_soul,
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

/// Bit position (0..=7) of a card-flag by name (e.g. `"drop_hold"`,
/// `"position_locked"`, `"dead"`). Returns `undefined` if no flag with
/// that name is declared in `cards/flags.json`. Throws on registry-build
/// failure. JS-side callers typically convert to a mask via
/// `1 << bit` before testing against `row.flags`.
#[wasm_bindgen(js_name = cardFlagBit)]
pub fn card_flag_bit(name: &str) -> Result<Option<u8>, JsValue> {
  core_card_flag_bit(name).map_err(|e| JsValue::from_str(&e))
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

/// Read the numeric value of a named trait off a packed card
/// definition. Returns `null` when:
/// - the trait name isn't in `traits.json`,
/// - the def doesn't carry that trait,
/// - or the packed def doesn't resolve to a registered card.
///
/// Source-of-truth pair with the server's
/// `def.trait_value(trait_id("name"))` path — both go through the
/// same `CardDefinition::trait_value` lookup, so client and server
/// agree on cost / speed numbers by construction.
///
/// Used by client A* (`pixijs/src/game/world/pathfind.ts`) to
/// resolve per-tile `cost` and per-soul `speed` for the step-time
/// calculation, mirroring the server validator in
/// `movement::move_soul_path`.
#[wasm_bindgen(js_name = traitValue)]
pub fn trait_value(packed_def: u16, name: &str) -> Result<Option<f32>, JsValue> {
  let trait_id = match crate::definition_core::trait_id(name)
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
  Ok(def.trait_value(trait_id))
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
  hex_stock0: u8,
  hex_stock1: u8,
  hex_has_stocks: u8,
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
  // `hex_has_stocks` is a 0/1 sentinel — `Option<(u8, u8)>` doesn't
  // round-trip through `wasm_bindgen` directly. Caller passes `0`
  // when the hex came from a Card row (no per-row stock; matcher
  // falls back to static aspects) and `1` when the hex came from a
  // synthetic tile slot (`stock0` / `stock1` are the per-tile u2s).
  let hex_stocks = if hex_has_stocks != 0 {
    Some((hex_stock0, hex_stock1))
  } else {
    None
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
    hex_stocks,
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

/// Compact JS-facing view of a recipe. `RecipeDef` itself isn't
/// `Serialize` (its `Entity` predicates are complex enums with
/// references), so the wasm boundary returns just the fields the
/// client typically needs.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RecipeBrief {
  /// Packed recipe id — what `proposeAction` takes as `recipeId`.
  recipe_index: u16,
  /// Top-level recipe-type discriminant: `"stack" | "magnetic" |
  /// "on_create"`. Combined with `direction`, the client knows which
  /// matcher to call.
  recipe_type: &'static str,
  /// `0 = up, 1 = down`. Only meaningful for `stack` / `magnetic`
  /// types; defaults to `0` for `on_create` (which has no direction).
  direction: u8,
  /// Number of slots the recipe declares. The client needs this many
  /// inventory cards (in declared order) to fill the recipe.
  slot_count: u32,
  /// Whether the recipe declares a root entity.
  has_root: bool,
  /// Whether the recipe declares a hex entity.
  has_hex: bool,
}

/// Look up a recipe by its tree-key (third-level key under
/// `<type>/<category>/<key>` in `recipes/data/*.json`). Returns a
/// `RecipeBrief`-shaped JS object on hit, `null` on miss. Throws on
/// registry-build failure.
///
/// Used by [`MagneticResolutionManager`](../../../../pixijs/src/game/magnetic/MagneticResolutionManager.ts)
/// to resolve a card def's `magneticRecipeKey` into the packed
/// recipe id needed for `proposeAction`, plus enough metadata
/// (slot count, direction) to drive client-side slot scanning.
#[wasm_bindgen(js_name = findRecipeByKey)]
pub fn find_recipe_by_key(key: &str) -> Result<JsValue, JsValue> {
  let opt = core_find_recipe(key).map_err(|e| JsValue::from_str(&e))?;
  match opt {
    Some(r) => {
      let (recipe_type, direction) = match r.recipe_type {
        crate::recipe_core::RecipeType::Stack(d) => {
          ("stack", direction_to_u8(d))
        }
        crate::recipe_core::RecipeType::Magnetic(d) => {
          ("magnetic", direction_to_u8(d))
        }
        crate::recipe_core::RecipeType::OnCreate => ("on_create", 0),
      };
      let brief = RecipeBrief {
        recipe_index: r.index,
        recipe_type,
        direction,
        slot_count: r.slots.len() as u32,
        has_root: r.root.is_some(),
        has_hex: r.hex.is_some(),
      };
      serde_wasm_bindgen::to_value(&brief)
        .map_err(|e| JsValue::from_str(&e.to_string()))
    }
    None => Ok(JsValue::NULL),
  }
}

fn direction_to_u8(d: StackDirection) -> u8 {
  match d {
    StackDirection::Up => 0,
    StackDirection::Down => 1,
  }
}

/// Try to match a magnetic recipe against `(root_def, slot_defs)`.
/// Mirrors the server-side `match_magnetic_recipe` (Phase 2 of the
/// magnetic rewrite). Returns a `StackMatch`-shaped JS object on
/// success or `null` if the predicates don't fit. Throws on
/// registry-build failure or invalid direction.
///
/// `direction` is `0 = up`, `1 = down`. The client looks up the
/// magnetic card's `magneticRecipeKey` to know the direction (the
/// recipe's `recipe_type` encodes it).
#[wasm_bindgen(js_name = matchMagneticRecipe)]
pub fn match_magnetic_recipe(
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
        "matchMagneticRecipe: invalid direction {} (expected 0 = up, 1 = down)",
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
  let opt = core_match_magnetic_recipe(root_def, &slot_defs, dir, Some(&has_candidates))
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

/// Every registered texture definition, in stable-id order. Each entry
/// carries `id`, `cardType`, `aspectId`, `aspectName`, `object`,
/// `size`, and `scale: { min, max }`. Returns an empty array when no
/// textures are registered. Throws on registry-build failure.
///
/// Called once at startup by `TextureRegistry.ts` to build the client-side
/// lookup map; not intended for per-frame use.
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
