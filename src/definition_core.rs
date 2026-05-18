//! Card and aspect definition registries.
//!
//! Decodes a `packed_definition` (`[card_type:u4][definition_id:u12]`)
//! into a `CardDefinition` carrying display name, color style, and aspect
//! list. Used by the action machinery, which evaluates recipes against the
//! aspects of the cards in a stack.
//!
//! # Loading
//!
//! Source data lives in `<repo>/content/`:
//!
//! - `cards/types.json` — registry of `card_type` ids.
//! - `aspects.json` — grouped aspect catalog. Aspects are 1-indexed in
//!   JSON insertion order across all groups (id 0 reserved as `ASPECT_NONE`).
//! - `cards/data/<card_type>/*.json` — per-file objects, each top-level
//!   key being a `card_type`, and that value an object of cards as
//!   `{ key: { "style": [c0, c1, c2],
//!             "aspects": { aspect_name: value, ... },
//!             "traits":  { trait_name:  value, ... } } }`.
//!
//! Card aspect names are translated to `AspectId`s at registry-build time;
//! `CardDefinition.aspects` carries `(AspectId, i32)` pairs for fast runtime
//! aggregation.
//!
//! `definition_id` is the 1-based position of a card within its bucket; 0
//! reserved as sentinel. `serde_json`'s `preserve_order` feature is enabled
//! so insertion order matches the JSON file.
//!
//! # Failure mode
//!
//! Each registry is built lazily on first access and stored in an
//! `OnceLock<Result<Registry, String>>`. If a build fails (malformed JSON,
//! unknown aspect referenced from card data, id out of range, etc.) the
//! error is **stored** in the cell — every subsequent accessor returns the
//! same `Err(_)` rather than re-running the build and re-paying the failure.
//! This avoids the panic-loop pattern an earlier version had.
//!
//! # Paths
//!
//! Singleton JSON catalogs (`aspects.json`, `cards/types.json`,
//! `cards/id.json`) are embedded directly with `include_str!`. The
//! per-file card buckets under `cards/data/*.json` are discovered at
//! compile time by `build.rs` and exposed as
//! [`crate::embedded_data::CARDS_FILES`]. Adding / renaming / removing
//! a file in `cards/data/` needs no source edit — cargo re-runs the
//! build script and the slice picks the change up.
//!
//! Each `cards/data/*.json` may be either a top-level array of buckets
//! or a single bare bucket object. Both shapes get normalised to a
//! bucket list inside [`build_cards`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde_json::Value;

use crate::flags_core::card_flag_bit;
use crate::packed::pack_definition;

// ---------- Aspects ----------

pub type AspectId = u8;

/// Sentinel id meaning "no aspect" / "unknown aspect". Aspect IDs are
/// 1-indexed.
pub const ASPECT_NONE: AspectId = 0;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Aspect {
  pub id: AspectId,
  /// Programmatic name from the JSON, e.g. `"combat"`.
  pub name: String,
  /// Human-readable description from the JSON.
  pub description: String,
  /// Unicode icon representing this aspect, e.g. `"⚔"`.
  pub icon: String,
  /// Display color packed as `0xRRGGBB`. Parsed from a `#RRGGBB`
  /// string in `aspects.json`. Sub-aspects inherit their parent's
  /// color when their JSON entry omits the field, same pattern as
  /// `icon` — lets families render with a single color across the
  /// tree (e.g. all corpus polarities share the body-red). Renderers
  /// use this directly: `gfx.fill({ color: aspect.color })`.
  pub color: u32,
  /// Top-level family name — the name of the aspect at the root of
  /// this aspect's parent chain. Top-level aspects have `group ==
  /// name`; nested sub-aspects carry their root ancestor's name
  /// (e.g. `berries.group == "food"` when berries is declared under
  /// fruit which is declared under food). Used for UI grouping
  /// (color coding pips, sectioning details panels) — categorical
  /// JSON-level grouping (`resources` / `elements` / ...) was
  /// dropped in favour of the implicit family-via-hierarchy.
  pub group: String,
  /// Parent aspect id when this aspect is nested under another in
  /// `aspects.json` (e.g. `apples` declared inside `food`). Forms a
  /// single-inheritance tree used by the recipe matcher to widen
  /// `Entity::Aspect` predicates: a recipe asking for `food` matches
  /// a card carrying any descendant of `food`. `None` for top-level
  /// aspects within their group.
  pub parent: Option<AspectId>,
}

struct AspectRegistry {
  by_id: Vec<Aspect>,                      // index by (id - 1)
  id_by_name: BTreeMap<String, AspectId>,
}

const ASPECTS_JSON: &str = include_str!("../cards/aspects.json");
static ASPECTS: OnceLock<Result<AspectRegistry, String>> = OnceLock::new();

fn aspects_registry() -> Result<&'static AspectRegistry, String> {
  ASPECTS.get_or_init(build_aspects).as_ref().map_err(|e| e.clone())
}

/// Look up an aspect's id by name. Returns `Ok(None)` if the registry built
/// successfully but no aspect with that name is declared, `Err` if the
/// aspect registry failed to build.
pub fn aspect_id(name: &str) -> Result<Option<AspectId>, String> {
  Ok(aspects_registry()?.id_by_name.get(name).copied())
}

/// Resolve an `AspectId` back to the full `Aspect` record. `Ok(None)` for
/// `ASPECT_NONE` and for ids past the end of the registry; `Err` on
/// registry-build failure.
pub fn aspect(id: AspectId) -> Result<Option<&'static Aspect>, String> {
  if id == ASPECT_NONE {
    return Ok(None);
  }
  Ok(aspects_registry()?.by_id.get((id - 1) as usize))
}

/// True iff `child` is `ancestor`, or `ancestor` appears anywhere
/// above `child` in the parent chain. Used by the recipe matcher
/// to widen `Entity::Aspect` predicates: a card carrying `berries`
/// satisfies `{aspect: food, min: 1}` because berries → fruit →
/// food puts food in berries' ancestor set. Returns false on
/// `ASPECT_NONE` for either argument or unknown ids. Registry
/// build failure surfaces as `Err`.
///
/// Walk depth is bounded by the aspect tree height (≤ 4 today;
/// soft cap of 16 enforced defensively in case a future content
/// change introduces a cycle that slipped past the registry-build
/// check). Same-id case is true (an aspect is trivially a
/// descendant of itself).
pub fn is_aspect_descendant(child: AspectId, ancestor: AspectId) -> Result<bool, String> {
  if child == ASPECT_NONE || ancestor == ASPECT_NONE {
    return Ok(false);
  }
  if child == ancestor {
    return Ok(true);
  }
  let registry = aspects_registry()?;
  let mut current = child;
  for _ in 0..16 {
    let Some(entry) = registry.by_id.get((current - 1) as usize) else {
      return Ok(false);
    };
    match entry.parent {
      None => return Ok(false),
      Some(p) if p == ancestor => return Ok(true),
      Some(p) => current = p,
    }
  }
  Ok(false)
}

/// All known aspects, ordered by id. `Err` on registry-build failure.
pub fn aspects() -> Result<&'static [Aspect], String> {
  Ok(&aspects_registry()?.by_id)
}

fn build_aspects() -> Result<AspectRegistry, String> {
  let root: Value = serde_json::from_str(ASPECTS_JSON)
    .map_err(|e| format!("aspects.json: parse failed: {}", e))?;
  let root = root
    .as_object()
    .ok_or_else(|| "aspects.json: top-level not an object".to_string())?;

  let mut by_id: Vec<Aspect> = Vec::new();
  let mut id_by_name: BTreeMap<String, AspectId> = BTreeMap::new();
  let mut next_id: u32 = 1;

  // Flat top-level — every non-`_` key is a top-level aspect entry.
  // Categorical grouping (resources / elements / ...) was retired in
  // favour of letting the aspect hierarchy carry the family info via
  // `Aspect.group`, which is set to the top-level ancestor's name
  // (so `berries.group == "food"` when berries is nested under
  // fruit which is nested under food).
  for (aspect_name, entry_value) in root {
    if aspect_name.starts_with('_') {
      continue;
    }
    let entry_obj = entry_value.as_object().ok_or_else(|| {
      format!(
        "aspects.json: aspect {:?} not an object (expected {{\"icon\": ..., \"description\": ...}})",
        aspect_name
      )
    })?;
    register_aspect_recursive(
      aspect_name,
      entry_obj,
      aspect_name,
      None,
      None,
      None,
      &mut by_id,
      &mut id_by_name,
      &mut next_id,
    )?;
  }

  Ok(AspectRegistry { by_id, id_by_name })
}

/// Register one aspect entry and recurse into any nested sub-aspects.
///
/// Property keys (`icon`, `description`) on the entry are read for
/// this aspect's own metadata; every *other* object-valued key is
/// treated as a sub-aspect with this aspect as its parent. `_`-
/// prefixed keys are skipped (the `_comment` convention). Scalar
/// values under unexpected keys reject — keeps typos visible
/// instead of silently dropped. The recursion is top-down so a
/// child's parent id is always already registered when we reach it.
///
/// `inherited_icon` / `inherited_color` are the values to fall back
/// on when this aspect omits its own `icon` / `color` — `None` at
/// the top level, and the parent aspect's values when recursing into
/// children. Lets sub-aspects collapse onto their parent's
/// visuals so callers can render whole families with a single glyph
/// and color while the JSON stays terse.
fn register_aspect_recursive(
  name: &str,
  entry: &serde_json::Map<String, Value>,
  group: &str,
  parent: Option<AspectId>,
  inherited_icon: Option<&str>,
  inherited_color: Option<u32>,
  by_id: &mut Vec<Aspect>,
  id_by_name: &mut BTreeMap<String, AspectId>,
  next_id: &mut u32,
) -> Result<(), String> {
  if *next_id > AspectId::MAX as u32 {
    return Err(format!(
      "aspects.json: more than {} aspects (id overflow)",
      AspectId::MAX,
    ));
  }
  let id = *next_id as AspectId;
  *next_id += 1;

  let description = entry
    .get("description")
    .and_then(Value::as_str)
    .ok_or_else(|| {
      format!(
        "aspects.json: aspect {}/{} missing or non-string 'description'",
        group, name
      )
    })?
    .to_string();

  let icon = match entry.get("icon").and_then(Value::as_str) {
    Some(s) => s.to_string(),
    None => match inherited_icon {
      Some(s) => s.to_string(),
      None => {
        return Err(format!(
          "aspects.json: aspect {}/{} missing 'icon' and has no parent to inherit from",
          group, name
        ));
      }
    },
  };

  let color = match entry.get("color") {
    Some(v) => {
      let s = v.as_str().ok_or_else(|| {
        format!(
          "aspects.json: aspect {}/{} 'color' not a string (expected '#RRGGBB')",
          group, name
        )
      })?;
      parse_hex_color(s).ok_or_else(|| {
        format!(
          "aspects.json: aspect {}/{} 'color' {:?} is not a valid '#RRGGBB' hex color",
          group, name, s
        )
      })?
    }
    None => match inherited_color {
      Some(c) => c,
      None => {
        return Err(format!(
          "aspects.json: aspect {}/{} missing 'color' and has no parent to inherit from",
          group, name
        ));
      }
    },
  };

  if id_by_name.contains_key(name) {
    return Err(format!(
      "aspects.json: aspect {:?} declared more than once",
      name
    ));
  }

  by_id.push(Aspect {
    id,
    name: name.to_string(),
    description,
    icon: icon.clone(),
    color,
    group: group.to_string(),
    parent,
  });
  id_by_name.insert(name.to_string(), id);

  // Walk object-valued keys as nested sub-aspects. Property keys
  // (`icon` / `description` / `color`) are scalars; `_`-prefixed
  // keys are documentation. Anything else with an object value is a
  // child; a non-object value under an unrecognised key is an
  // authoring error and rejects.
  for (sub_name, sub_value) in entry {
    if sub_name.starts_with('_') {
      continue;
    }
    if sub_name == "icon" || sub_name == "description" || sub_name == "color" {
      continue;
    }
    let sub_obj = sub_value.as_object().ok_or_else(|| {
      format!(
        "aspects.json: aspect {}/{}: unexpected key {:?} (not a property name and value is not a sub-aspect object)",
        group, name, sub_name
      )
    })?;
    register_aspect_recursive(
      sub_name,
      sub_obj,
      group,
      Some(id),
      Some(&icon),
      Some(color),
      by_id,
      id_by_name,
      next_id,
    )?;
  }
  Ok(())
}

/// Parse a `"#RRGGBB"` string into a `0xRRGGBB` `u32`. Returns
/// `None` for malformed input — caller decides how to report the
/// error so the message can name the offending aspect.
fn parse_hex_color(s: &str) -> Option<u32> {
  let bytes = s.as_bytes();
  if bytes.len() != 7 || bytes[0] != b'#' {
    return None;
  }
  u32::from_str_radix(&s[1..], 16).ok()
}

// ---------- Traits ----------

pub type TraitId = u8;

/// Sentinel id meaning "no trait" / "unknown trait". Trait IDs are
/// 1-indexed, matching the aspect convention.
pub const TRAIT_NONE: TraitId = 0;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Trait {
  pub id: TraitId,
  /// Programmatic name from the JSON.
  pub name: String,
  /// Human-readable description from the JSON.
  pub description: String,
  /// Group the trait was declared under, e.g. `"general"`.
  pub group: String,
}

struct TraitRegistry {
  by_id: Vec<Trait>,                       // index by (id - 1)
  id_by_name: BTreeMap<String, TraitId>,
}

const TRAITS_JSON: &str = include_str!("../cards/traits.json");
static TRAITS: OnceLock<Result<TraitRegistry, String>> = OnceLock::new();

fn traits_registry() -> Result<&'static TraitRegistry, String> {
  TRAITS.get_or_init(build_traits).as_ref().map_err(|e| e.clone())
}

/// Look up a trait's id by name. Returns `Ok(None)` if the registry built
/// successfully but no trait with that name is declared, `Err` if the
/// trait registry failed to build.
pub fn trait_id(name: &str) -> Result<Option<TraitId>, String> {
  Ok(traits_registry()?.id_by_name.get(name).copied())
}

/// Resolve a `TraitId` back to the full `Trait` record. `Ok(None)` for
/// `TRAIT_NONE` and for ids past the end of the registry; `Err` on
/// registry-build failure. Named `trait_def` rather than `trait` because
/// `trait` is a Rust keyword.
pub fn trait_def(id: TraitId) -> Result<Option<&'static Trait>, String> {
  if id == TRAIT_NONE {
    return Ok(None);
  }
  Ok(traits_registry()?.by_id.get((id - 1) as usize))
}

/// All known traits, ordered by id. `Err` on registry-build failure.
pub fn traits() -> Result<&'static [Trait], String> {
  Ok(&traits_registry()?.by_id)
}

fn build_traits() -> Result<TraitRegistry, String> {
  let root: Value = serde_json::from_str(TRAITS_JSON)
    .map_err(|e| format!("traits.json: parse failed: {}", e))?;
  let root = root
    .as_object()
    .ok_or_else(|| "traits.json: top-level not an object".to_string())?;

  let mut by_id: Vec<Trait> = Vec::new();
  let mut id_by_name: BTreeMap<String, TraitId> = BTreeMap::new();
  let mut next_id: u32 = 1;

  for (group_name, group_value) in root {
    if group_name.starts_with('_') {
      continue;
    }
    let group_obj = group_value.as_object().ok_or_else(|| {
      format!("traits.json: group {:?} not an object", group_name)
    })?;

    for (trait_name, desc_value) in group_obj {
      if trait_name.starts_with('_') {
        continue;
      }
      if next_id > TraitId::MAX as u32 {
        return Err(format!(
          "traits.json: more than {} traits (id overflow)",
          TraitId::MAX,
        ));
      }
      let id = next_id as TraitId;
      next_id += 1;

      let description = desc_value
        .as_str()
        .ok_or_else(|| {
          format!(
            "traits.json: trait {}/{} description not a string",
            group_name, trait_name
          )
        })?
        .to_string();

      by_id.push(Trait {
        id,
        name: trait_name.clone(),
        description,
        group: group_name.clone(),
      });
      id_by_name.insert(trait_name.clone(), id);
    }
  }

  Ok(TraitRegistry { by_id, id_by_name })
}

// ---------- Cards ----------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDefinition {
  pub card_type: u8,
  /// 1-based id within the type's bucket. Widened to u16 (from u8)
  /// when the `card_category` dimension was retired — `packed_definition`'s
  /// 4-bit category slot collapsed into `def_id`, giving 4095 distinct
  /// def_ids per type. See docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md.
  pub definition_id: u16,
  /// Programmatic key from the JSON, e.g. `"axe"`. Stable identifier
  /// used as the path-form last segment (`requisite/default/axe`) and
  /// as the lookup key in `content/locales/cards/<lang>.json` for
  /// display label / description resolution. Display labels are NOT
  /// stored on the definition — clients resolve them via the locales
  /// registry; the bare key is the dev-side fallback.
  pub key: String,
  /// Style array: exactly 3 entries — `#RRGGBB` color codes for
  /// primary, secondary, outline. Sprite filenames previously lived
  /// here at indices 3-4; they now live on the top-level [`sprite`]
  /// field. Validated at build time.
  pub style: Vec<String>,
  /// Optional sprite filename rendered centred on the card body
  /// (rect cards) or as the foreground overlay (hex cards). Resolved
  /// at runtime against `public/textures/cards/objects/<filename>`.
  /// `None` (or missing in JSON) means "no sprite."
  pub sprite: Option<String>,
  /// `(aspect_id, value)` pairs. Names are translated to ids at registry
  /// build time via `aspect_id`; an unknown aspect name in card data is a
  /// stored registry-build error. Each `aspect_id` appears at most once
  /// per definition.
  pub aspects: Vec<(AspectId, i32)>,
  /// `(trait_id, value)` pairs. Same shape as `aspects` but resolved
  /// against `traits.json` via `trait_id`. Empty for cards that don't
  /// declare any traits yet (the JSON requires the field to be present
  /// as an object, but `{}` is valid).
  /// `(trait_id, value)` pairs parsed from the card's JSON `"traits"`
  /// block. Values are `f32` because some traits (notably the tile
  /// `cost` trait used by movement pathfinding) are naturally
  /// fractional — `forest_2`'s `1.2` cost wouldn't survive an `i32`
  /// round-trip. JSON integers parse to whole-number floats (`1` →
  /// `1.0`) without loss.
  pub traits: Vec<(TraitId, f32)>,
  /// Bit-mask of flags applied to every card spawned with this
  /// definition. Currently always 0 — per-definition flag presets are
  /// not declared in card JSON. Kept on the struct because
  /// `cards::create` / `cards::create_at` on the server still OR this
  /// mask into the row's `flags` column; reintroduce JSON-driven
  /// initialisation here when a definition needs to spawn cards with
  /// non-zero flags again.
  pub flags: u32,
  /// Lifecycle-resolution recipe id, by stable string key. `Some`
  /// only for cards with a queued transformation (magnetic-style
  /// anchors AND decay-style cards like `corpus-`). Stored as a
  /// string here — and not the packed `u16` — to avoid a
  /// build-order cycle between the cards and recipes registries
  /// (recipes resolve card paths during their build; if cards
  /// resolved recipe ids during their build, we'd deadlock).
  /// Resolve to a packed id at use time via
  /// [`lifecycle_recipe_for_def`]. See
  /// [docs/LIFECYCLE_REWRITE.md](../../../docs/LIFECYCLE_REWRITE.md).
  pub lifecycle_recipe_key: Option<String>,
  /// Phase duration in milliseconds. `Some` only for lifecycle-
  /// pending cards. A card's lifecycle phase ends at
  /// `install_row.valid_at_time + lifecycle_duration_ms`; computed
  /// (never stamped as a future row).
  pub lifecycle_duration_ms: Option<u32>,
  /// Row-mutable aspect slots. Each entry pins one aspect that
  /// carries a *per-row* value (independent from the static
  /// `aspects` field above, which is def-bound). Tile defs that
  /// declare `stock` get per-tile bits in the `Zone` row; recipes
  /// matching tile-rooted entities read these bits instead of the
  /// def's static aspect value.
  ///
  /// Capped at 2 slots per def — the per-tile u16 has room for
  /// two u2 stock values. See
  /// [docs/TILE_ASPECTS.md](../../../docs/TILE_ASPECTS.md).
  ///
  /// Order matters: the first entry maps to the row's stock-slot
  /// 0, the second to stock-slot 1. Don't reorder once data exists
  /// — same rationale as `aspects.json` id stability.
  pub stock: Vec<StockSlot>,
}

/// Tag for which climate axis a stock slot couples to. Stored on
/// [`StockSlot::climate_axis`]; `None` means "no coupling, fall back
/// to an independent per-slot noise band at worldgen time."
///
/// Index value (`as u8`) matches the `AXIS_*` constants in
/// `world_gen.rs` — kept in lockstep so a stock slot's
/// `climate_axis` can index a `Climate` `[f32; 4]` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClimateAxis {
  Elevation = 0,
  Temperature = 1,
  Humidity = 2,
  Aether = 3,
}

impl ClimateAxis {
  /// Parse a JSON axis name (lowercase) into the enum. `None` for
  /// unknown names — the parser surfaces it as a descriptive error.
  pub fn from_name(name: &str) -> Option<Self> {
    match name {
      "elevation" => Some(Self::Elevation),
      "temperature" => Some(Self::Temperature),
      "humidity" => Some(Self::Humidity),
      "aether" => Some(Self::Aether),
      _ => None,
    }
  }

  /// Index into a `[f32; 4]` climate sample bundle. Matches the
  /// `AXIS_*` constants in `world_gen.rs`.
  pub fn index(self) -> usize {
    self as usize
  }
}

/// One row-mutable aspect slot declared on a `CardDefinition`. The
/// def fixes the aspect identity + bounds; the row carries the
/// current value in 2 bits of the per-tile u16 (mask `0x3` after
/// the appropriate shift).
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockSlot {
  /// Aspect this slot tracks. Resolved at registry build time.
  pub aspect_id: AspectId,
  /// Maximum value the row can store for this slot. Hard-capped at
  /// 3 (the u2 width). A def may declare `max: 1` to use the slot
  /// as a presence boolean.
  pub max: u8,
  /// Initial value `cards::create` / mini_zone bootstrap / recipe-
  /// spawned tiles set the slot to. Bounded by `max`.
  ///
  /// **Worldgen ignores `default`** — `world_gen::pick_stocks_for`
  /// samples values from climate or an independent noise band. The
  /// field is the value for tiles constructed *outside* the
  /// climate-driven world generator (manual `assign_tile`, recipe
  /// `ProductPlace::Location`, etc).
  pub default: u8,
  /// Optional climate-axis coupling. When `Some`, worldgen reads
  /// that axis's sample for the cell, remaps through
  /// `climate_axis_min`/`climate_axis_max`, and quantises into
  /// `[0..=max]`. When `None`, worldgen falls back to an
  /// independent per-slot FBM band keyed by
  /// `(global_q, global_r, seed + slot_index)`.
  ///
  /// Lets authors say "wood density tracks humidity" or "stone
  /// density tracks elevation" without burning a noise channel per
  /// slot. See [`docs/BIOME_GENERATION.md`](../../../docs/BIOME_GENERATION.md).
  pub climate_axis: Option<ClimateAxis>,
  /// Lower edge of the climate window when `climate_axis` is `Some`.
  /// Samples below this clamp to 0 stock. Default 0.0 (use the
  /// entire axis range).
  pub climate_axis_min: f32,
  /// Upper edge of the climate window. Samples above this clamp to
  /// `max` stock. Default 1.0.
  pub climate_axis_max: f32,
}

impl CardDefinition {
  /// Look up a trait's value on this definition by `trait_id`.
  /// Returns `None` if the card doesn't carry that trait — callers
  /// supply a per-trait default. Used by `movement::tile_cost` to
  /// read the `cost` trait off a tile def, where "no trait" means
  /// "default cost." */
  pub fn trait_value(&self, trait_id: TraitId) -> Option<f32> {
    self
      .traits
      .iter()
      .find_map(|(id, v)| (*id == trait_id).then_some(*v))
  }

  /// Index (0 or 1) of the stock slot tracking `aspect_id` on this
  /// definition, or `None` if no slot tracks it. Used by the recipe
  /// matcher to know which u2 of a tile's row stock holds the
  /// current value when matching an `Aspect` predicate against a
  /// tile-rooted entity, and by `action_completion::apply` when
  /// decrementing on `consume`.
  pub fn stock_slot_index(&self, aspect_id: AspectId) -> Option<usize> {
    self
      .stock
      .iter()
      .position(|slot| slot.aspect_id == aspect_id)
  }

  /// Stock slot index for `aspect_id` with sub-aspect widening:
  /// prefers an exact `aspect_id` match, falls back to the first
  /// slot whose declared aspect descends from `aspect_id` in the
  /// aspect tree. Pairs with `entity_specificity_with_stocks`'s
  /// matcher widening — a recipe declaring `consume.hex.aspect.wood`
  /// against a forest tile whose only stock slot is `pine` (pine
  /// descends from wood) resolves to that pine slot.
  ///
  /// Returns `None` only when no slot is an exact or descendant
  /// match. Multi-descendant tiles (e.g. hypothetical `pine` +
  /// `oak`) resolve to the first declared slot; callers performing
  /// `Sub` ops that need to drain multiple descendant slots should
  /// use [`Self::descendant_stock_slot_indices`] instead.
  pub fn widened_stock_slot_index(&self, aspect_id: AspectId) -> Option<usize> {
    // Exact match wins — preserves the simple semantics when the
    // recipe names the same aspect the tile declares.
    if let Some(idx) = self.stock_slot_index(aspect_id) {
      return Some(idx);
    }
    self.stock.iter().position(|slot| {
      is_aspect_descendant(slot.aspect_id, aspect_id).unwrap_or(false)
    })
  }

  /// All stock slot indices on this def whose declared aspect
  /// descends from (or equals) `aspect_id`, in declaration order.
  /// Used by the consume path to decrement aspects from a multi-
  /// descendant tile (e.g. drain `pine` first, then `oak`, when a
  /// recipe consumes `wood`). Empty vec when no slot is a match.
  pub fn descendant_stock_slot_indices(&self, aspect_id: AspectId) -> Vec<usize> {
    self
      .stock
      .iter()
      .enumerate()
      .filter_map(|(idx, slot)| {
        if is_aspect_descendant(slot.aspect_id, aspect_id).unwrap_or(false) {
          Some(idx)
        } else {
          None
        }
      })
      .collect()
  }
}

const CARD_TYPES_JSON: &str = include_str!("../cards/types.json");
const CARD_IDS_JSON: &str = include_str!("../cards/id.json");

/// Maximum valid id for a `card_type`. Occupies the top u4 of
/// `packed_definition`, so 0xF is the hard cap.
const MAX_TYPE_ID: u64 = 0xF;
/// Maximum valid `definition_id` — fits in the low u12 of
/// `packed_definition` after the category retire.
const MAX_DEFINITION_ID: u64 = 0x0FFF;

use crate::embedded_data::CARDS_FILES;

struct CardRegistry {
  by_packed: BTreeMap<u16, CardDefinition>,
  /// `(type_id, key)` → `packed_definition`.
  by_path: BTreeMap<(u8, String), u16>,
  /// Bare key → `packed_definition`, from `cards/id.json`.
  by_key: BTreeMap<String, u16>,
  type_ids: BTreeMap<String, u8>,
  /// `type_id` → shape (`"rect"` or `"hex"`) from `cards/types.json`.
  /// Drives [`is_hex_type`]; missing types default to `"rect"`.
  type_shapes: BTreeMap<u8, String>,
  /// Reverse of `type_ids`: `type_id` → name. Used to construct locale
  /// paths like `"requisite.axe"` from a packed definition.
  type_names: BTreeMap<u8, String>,
}

static CARDS: OnceLock<Result<CardRegistry, String>> = OnceLock::new();

fn cards_registry() -> Result<&'static CardRegistry, String> {
  CARDS.get_or_init(build_cards).as_ref().map_err(|e| e.clone())
}

/// Look up the `CardDefinition` for a `packed_definition`. `Ok(None)` for
/// the sentinel value 0, unknown `card_type`, or a `definition_id` past
/// the end of its bucket. `Err` on registry-build failure.
pub fn decode_definition(packed: u16) -> Result<Option<&'static CardDefinition>, String> {
  Ok(cards_registry()?.by_packed.get(&packed))
}

/// Every card def of the given `card_type`. Allocates a fresh `Vec`
/// per call — fine for worldgen-rate use (tens of definitions, called
/// per-cell on a procedural pass) but cache if it becomes a hot path.
/// `Err` on registry-build failure.
///
/// Used by `world_gen.rs` to walk every `tile` (card_type 7) def when
/// scoring climate envelopes against a cell's climate samples.
pub fn cards_of_type(card_type: u8) -> Result<Vec<&'static CardDefinition>, String> {
  Ok(
    cards_registry()?
      .by_packed
      .values()
      .filter(|d| d.card_type == card_type)
      .collect(),
  )
}

/// Look up a card's `packed_definition` by its bare key (e.g. `"fatigue"`).
/// Uses the stable mapping from `cards/id.json` — O(log n), no scan needed.
/// Returns `Ok(None)` if the registry built but no card with that key exists;
/// `Err` on registry-build failure.
pub fn find_packed_by_key(card_key: &str) -> Result<Option<u16>, String> {
  Ok(cards_registry()?.by_key.get(card_key).copied())
}

/// Whether the given `card_type` id resolves to a hex-shaped type
/// (`"hex"` in `cards/types.json`). Used by `magnetic.rs` to decide
/// whether the action's actor is a hex anchor and slot[0] should be
/// attached as a hex-root rather than stacked top/bottom. Unknown
/// type ids default to `false` (rect-like) so a stale `packed_definition`
/// can't accidentally trip hex-specific paths.
pub fn is_hex_type(type_id: u8) -> Result<bool, String> {
  Ok(cards_registry()?
    .type_shapes
    .get(&type_id)
    .map_or(false, |s| s == "hex"))
}

/// All known `card_type` ids keyed by name. Recipe parsing resolves
/// `"@<type>"` entity strings through this map. Triggers a build of the
/// card registry on first access.
pub fn card_type_ids() -> Result<&'static BTreeMap<String, u8>, String> {
  Ok(&cards_registry()?.type_ids)
}

/// Build the dotted locale-lookup path for a packed definition, e.g.
/// `"requisite.axe"`. Returns `Ok(None)` for unknown packed ids.
/// The path format mirrors `locales/cards/<lang>.json`'s nesting and is
/// consumed by `locales_core::label("cards", lang, path)`.
pub fn card_locale_path(packed_def: u16) -> Result<Option<String>, String> {
  let registry = cards_registry()?;
  let Some(def) = registry.by_packed.get(&packed_def) else {
    return Ok(None);
  };
  let type_name = registry
    .type_names
    .get(&def.card_type)
    .map(String::as_str)
    .unwrap_or("unknown");
  Ok(Some(format!("{}.{}", type_name, def.key)))
}

/// Resolve a `"type/key"` string to the card's `packed_definition`.
/// Returns a descriptive `Err` for malformed paths, unrecognized
/// type or key, or registry-build failure.
///
/// `"type/category/key"` (legacy three-segment) is still accepted with
/// the middle segment ignored, easing the transition from the
/// category-retired data files; remove this tolerance once any
/// remaining authored content has flattened.
pub fn find_packed(card_path: &str) -> Result<u16, String> {
  let parts: Vec<&str> = card_path.split('/').collect();
  let (type_name, card_key) = match parts.len() {
    2 => (parts[0], parts[1]),
    3 => (parts[0], parts[2]), // legacy: type/category/key — middle ignored
    _ => {
      return Err(format!(
        "invalid card path {:?}, expected 'type/key'",
        card_path
      ));
    }
  };

  let registry = cards_registry()?;
  let &type_id = registry
    .type_ids
    .get(type_name)
    .ok_or_else(|| format!("unknown card type {:?}", type_name))?;
  registry
    .by_path
    .get(&(type_id, card_key.to_string()))
    .copied()
    .ok_or_else(|| format!("unknown card {:?}", card_path))
}

// ---------- Lifecycle-recipe resolution ----------
//
// Card defs store `lifecycle_recipe_key` as a string (the third-level
// recipe key from the JSON tree) rather than the packed `u16` id, to
// avoid a build-order cycle with the recipes registry. The helpers
// below do the lookup at use time. They live here rather than in
// `recipe_core` so the API hangs off the card-definition surface
// callers naturally hold.

/// Look up the packed `u16` id of the lifecycle recipe declared on
/// this definition. Returns:
///
/// - `Ok(None)` — the def has no `lifecycle_recipe_key` (it's not a
///   lifecycle-pending card).
/// - `Ok(Some(packed_id))` — found and the recipe is one of the
///   magnetic types (today; lifecycle rewrite phase 6 folds these
///   into `Stack(_)`).
/// - `Err` — the def declares a recipe key that doesn't exist in the
///   recipes registry, or the recipe exists but isn't a magnetic
///   type, or the recipes registry failed to build.
///
/// Forces the recipes registry to build on first call. Callers in hot
/// paths may want to cache the result.
pub fn lifecycle_recipe_for_def(def: &CardDefinition) -> Result<Option<u16>, String> {
  let Some(recipe_key) = def.lifecycle_recipe_key.as_deref() else {
    return Ok(None);
  };
  let recipe = crate::recipe_core::find_recipe(recipe_key)?.ok_or_else(|| {
    format!(
      "card {:?}: lifecycle.recipe {:?} not declared in any recipe file",
      def.key, recipe_key
    )
  })?;
  let rt = recipe.recipe_type;
  if !matches!(rt, crate::recipe_core::RecipeType::Magnetic(_)) {
    return Err(format!(
      "card {:?}: lifecycle.recipe {:?} resolves to a non-magnetic recipe (type {:?})",
      def.key, recipe_key, rt
    ));
  }
  Ok(Some(recipe.index))
}

/// Walk every registered card definition and validate that any
/// declared `lifecycle_recipe_key` resolves to a real lifecycle recipe.
/// Returns `Ok(())` if every lifecycle card checks out, or a
/// descriptive error on the first failure.
///
/// Designed for build-time / startup validation — `bin/content check`
/// runs this so authoring errors surface before deployment rather
/// than at first lifecycle-card spawn. Idempotent and free to call
/// from tests.
pub fn validate_lifecycle_recipes() -> Result<(), String> {
  let registry = cards_registry()?;
  for def in registry.by_packed.values() {
    if def.lifecycle_recipe_key.is_none() && def.lifecycle_duration_ms.is_none() {
      continue;
    }
    // A half-specified lifecycle block is parser-rejected, but
    // belt-and-suspenders the check here too.
    if def.lifecycle_recipe_key.is_none() || def.lifecycle_duration_ms.is_none() {
      return Err(format!(
        "card {:?}: lifecycle block half-specified \
         (recipe_key={:?}, duration_ms={:?})",
        def.key, def.lifecycle_recipe_key, def.lifecycle_duration_ms
      ));
    }
    lifecycle_recipe_for_def(def)?;
  }
  Ok(())
}

fn build_cards() -> Result<CardRegistry, String> {
  let types_root: Value = serde_json::from_str(CARD_TYPES_JSON)
    .map_err(|e| format!("cards/types.json: parse failed: {}", e))?;

  let type_ids = json_id_map(&types_root, "types")?;
  let type_shapes = json_type_shapes(&types_root)?;
  let type_names: BTreeMap<u8, String> =
    type_ids.iter().map(|(k, &v)| (v, k.clone())).collect();

  // Load stable definition_id map — must exist (run gen-ids.py before building).
  // Format: { "<card_type>": { "<key>": <definition_id>, ... }, ... }
  // (The `category` middle level was retired — see
  // docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md.)
  let id_root: Value = serde_json::from_str(CARD_IDS_JSON)
    .map_err(|e| format!("cards/id.json: parse failed: {}", e))?;
  let id_obj = id_root
    .as_object()
    .ok_or_else(|| "cards/id.json: top-level not an object".to_string())?;
  let mut definition_ids: BTreeMap<String, BTreeMap<String, u16>> = BTreeMap::new();
  for (type_name, type_val) in id_obj {
    let type_obj = type_val
      .as_object()
      .ok_or_else(|| format!("cards/id.json: entry for type {:?} not an object", type_name))?;
    let mut inner: BTreeMap<String, u16> = BTreeMap::new();
    for (key, val) in type_obj {
      let n = val.as_u64().ok_or_else(|| {
        format!("cards/id.json: definition_id for {:?}/{:?} not an integer", type_name, key)
      })?;
      if n == 0 || n > MAX_DEFINITION_ID {
        return Err(format!(
          "cards/id.json: definition_id {} for {:?}/{:?} out of range (1..={})",
          n, type_name, key, MAX_DEFINITION_ID
        ));
      }
      inner.insert(key.clone(), n as u16);
    }
    definition_ids.insert(type_name.clone(), inner);
  }

  let mut by_packed: BTreeMap<u16, CardDefinition> = BTreeMap::new();
  let mut by_path: BTreeMap<(u8, String), u16> = BTreeMap::new();
  let mut by_key: BTreeMap<String, u16> = BTreeMap::new();

  for (filename, content) in CARDS_FILES {
    let parsed: Value = serde_json::from_str(content)
      .map_err(|e| format!("{}: parse failed: {}", filename, e))?;
    // Nested shape:
    //   {
    //     "<card_type>": {
    //       "<key>": { "style": [...], "aspects"?: {...}, "traits"?: {...} }
    //     }
    //   }
    //
    // Multiple types per file → multiple top-level keys.
    // `aspects` / `traits` are optional (default `{}`); `style` is
    // required. Card paths matching `cards/types.json` decode to ids
    // here; unknown types are silently skipped so card data can
    // outpace the registry without breaking the build (a typo just
    // won't produce a decodable card).
    let root = parsed.as_object().ok_or_else(|| {
      format!("{}: top-level must be an object keyed by card_type", filename)
    })?;

    for (type_name, by_key_val) in root {
      let Some(&card_type) = type_ids.get(type_name) else { continue };
      let cards_obj = by_key_val.as_object().ok_or_else(|| {
        format!("{}: type {:?}: value not an object", filename, type_name)
      })?;

      for (key, value) in cards_obj.iter() {
        let definition_id = definition_ids
          .get(type_name)
          .and_then(|m| m.get(key.as_str()))
          .copied()
          .ok_or_else(|| {
            format!(
              "{}: card {:?} (type {:?}) not found in cards/id.json — run gen-ids.py",
              filename, key, type_name
            )
          })?;
        let definition = parse_card(filename, value, card_type, definition_id, key)?;
        let packed = pack_definition(card_type, definition_id);
        by_packed.insert(packed, definition);
        by_path.insert((card_type, key.clone()), packed);
        by_key.insert(key.clone(), packed);
      }
    }
  }

  Ok(CardRegistry { by_packed, by_path, by_key, type_ids, type_shapes, type_names })
}

/// Build a `type_id → shape` map from `cards/types.json`'s `types`
/// section. Skips reserved/comment keys and entries without a `shape`
/// field. Mirrors the structure of [`json_id_map`] but pulls a
/// different field.
fn json_type_shapes(root: &Value) -> Result<BTreeMap<u8, String>, String> {
  let types_obj = root
    .get("types")
    .and_then(Value::as_object)
    .ok_or_else(|| "cards/types.json: 'types' missing or not an object".to_string())?;
  let mut result = BTreeMap::new();
  for (name, info) in types_obj {
    if name.starts_with('_') {
      continue;
    }
    let id = info
      .get("id")
      .and_then(Value::as_u64)
      .ok_or_else(|| format!("cards/types.json: types.{:?} missing 'id'", name))?;
    if id > MAX_TYPE_ID {
      continue;
    }
    if let Some(shape) = info.get("shape").and_then(Value::as_str) {
      result.insert(id as u8, shape.to_string());
    }
  }
  Ok(result)
}

/// Build a `name → id` map from a section of `cards/types.json`.
///
/// Skips keys that begin with `_` (these are comments / placeholder
/// reservations like `_reserved_1`). Real entries — i.e. those whose key
/// doesn't start with `_` — must carry a numeric `id` field in `[0, 0xF]`;
/// missing or out-of-range ids are an error rather than a silent drop, so a
/// typo'd field name fails loudly.
fn json_id_map(root: &Value, section: &str) -> Result<BTreeMap<String, u8>, String> {
  let section_obj = root
    .get(section)
    .and_then(Value::as_object)
    .ok_or_else(|| format!("cards/types.json: '{}' missing or not an object", section))?;

  let mut result = BTreeMap::new();
  for (name, info) in section_obj {
    if name.starts_with('_') {
      continue;
    }
    let id_value = info.get("id").ok_or_else(|| {
      format!("cards/types.json: '{}' entry {:?} missing 'id'", section, name)
    })?;
    let id_u64 = id_value.as_u64().ok_or_else(|| {
      format!(
        "cards/types.json: '{}' entry {:?} 'id' not a non-negative integer",
        section, name
      )
    })?;
    if id_u64 > MAX_TYPE_ID {
      return Err(format!(
        "cards/types.json: '{}' entry {:?} id {} exceeds u4 max ({})",
        section, name, id_u64, MAX_TYPE_ID,
      ));
    }
    result.insert(name.clone(), id_u64 as u8);
  }
  Ok(result)
}

fn parse_card(
  filename: &str,
  value: &Value,
  card_type: u8,
  definition_id: u16,
  key: &str,
) -> Result<CardDefinition, String> {
  let obj = value
    .as_object()
    .ok_or_else(|| format!("{}: card {}: spec not an object", filename, key))?;

  let style_arr = obj
    .get("style")
    .and_then(Value::as_array)
    .ok_or_else(|| format!("{}: card {}: missing or non-array 'style'", filename, key))?;
  if style_arr.len() != 3 {
    return Err(format!(
      "{}: card {}: style needs exactly 3 entries (#RRGGBB colors)",
      filename, key
    ));
  }
  let style: Vec<String> = vec![
    style_str(filename, key, style_arr, 0)?,
    style_str(filename, key, style_arr, 1)?,
    style_str(filename, key, style_arr, 2)?,
  ];

  // Optional top-level sprite filename. Replaces the legacy
  // `style[3]` / `style[4]` slots; rendered centred on the card.
  let sprite = match obj.get("sprite") {
    None => None,
    Some(Value::String(s)) if s.is_empty() => None,
    Some(Value::String(s)) => Some(s.clone()),
    Some(_) => {
      return Err(format!(
        "{}: card {}: 'sprite' must be a string filename",
        filename, key
      ));
    }
  };

  // `aspects` and `traits` are optional. Empty / missing both mean
  // "no aspects" / "no traits" — most cards declare neither and the
  // tree-shaped data file lets them omit the empty objects entirely.
  // When present, the value must be an object; non-object → parse error.
  let aspects = match obj.get("aspects") {
    None => Vec::new(),
    Some(Value::Object(aspects_obj)) => parse_aspects(filename, key, aspects_obj)?,
    Some(_) => {
      return Err(format!("{}: card {}: 'aspects' not an object", filename, key));
    }
  };
  let traits = match obj.get("traits") {
    None => Vec::new(),
    Some(Value::Object(traits_obj)) => parse_traits(filename, key, traits_obj)?,
    Some(_) => {
      return Err(format!("{}: card {}: 'traits' not an object", filename, key));
    }
  };

  // `lifecycle` block (or its `magnetic` alias for backwards compat
  // during the lifecycle rewrite): optional. When present, declares
  // this card as a lifecycle-pending card that resolves via a
  // specific recipe over a fixed duration. Both `recipe` and
  // `duration_ms` are required if the block appears at all — a
  // half-specified block is a parse error rather than a silent
  // default. See [docs/LIFECYCLE_REWRITE.md](../../../docs/LIFECYCLE_REWRITE.md).
  let lifecycle_block = obj.get("lifecycle").or_else(|| obj.get("magnetic"));
  let (lifecycle_recipe_key, lifecycle_duration_ms) = match lifecycle_block {
    None => (None, None),
    Some(Value::Object(mag_obj)) => parse_lifecycle(filename, key, mag_obj)?,
    Some(_) => {
      return Err(format!(
        "{}: card {}: 'lifecycle' (or 'magnetic') not an object",
        filename, key
      ));
    }
  };

  // Cards declared as lifecycle-pending auto-inherit the `magnetic`
  // flag bit (cards/flags.json bit 12 — name kept for stable-id
  // discipline; phase 6 of the lifecycle rewrite will rename it to
  // `lifecycle_pending`). The card-write hook in `cards::write_at`
  // keys off this bit to install the lifecycle_pending row.
  let mut flags: u32 = 0;
  if lifecycle_recipe_key.is_some() {
    let bit = card_flag_bit("magnetic")?.ok_or_else(|| {
      "cards/flags.json missing single-bit flag 'magnetic' — required by \
       lifecycle def-flag inheritance"
        .to_string()
    })?;
    flags |= 1u32 << bit;
  }

  // `stock` block: optional, declares row-mutable aspect slots on
  // this card. See [docs/TILE_ASPECTS.md](../../../docs/TILE_ASPECTS.md).
  let stock = match obj.get("stock") {
    None => Vec::new(),
    Some(Value::Array(arr)) => parse_stock(filename, key, arr)?,
    Some(_) => {
      return Err(format!(
        "{}: card {}: 'stock' must be an array of slot objects",
        filename, key
      ));
    }
  };

  Ok(CardDefinition {
    card_type,
    definition_id,
    key: key.to_string(),
    style,
    sprite,
    aspects,
    traits,
    flags,
    lifecycle_recipe_key,
    lifecycle_duration_ms,
    stock,
  })
}

/// Maximum value a stock slot can take. u2 width (0..=3); the
/// per-tile u16 has room for two u2 slots.
const MAX_STOCK_VALUE: u8 = 3;

/// Maximum number of stock slots a single card def can declare.
/// The per-tile u16 packs `[def_id:u12 | stock0:u2 | stock1:u2]`,
/// so two slots is the hard cap. Future schema widenings could
/// raise this; v1 caps strictly.
const MAX_STOCK_SLOTS: usize = 2;

fn parse_stock(
  filename: &str,
  key: &str,
  arr: &[Value],
) -> Result<Vec<StockSlot>, String> {
  if arr.len() > MAX_STOCK_SLOTS {
    return Err(format!(
      "{}: card {}: 'stock' has {} slots; maximum is {}",
      filename, key, arr.len(), MAX_STOCK_SLOTS
    ));
  }
  let mut out: Vec<StockSlot> = Vec::with_capacity(arr.len());
  let mut seen: BTreeSet<AspectId> = BTreeSet::new();
  for (i, entry) in arr.iter().enumerate() {
    let obj = entry.as_object().ok_or_else(|| {
      format!("{}: card {}: stock[{}] not an object", filename, key, i)
    })?;

    let aspect_name = obj
      .get("aspect")
      .and_then(Value::as_str)
      .ok_or_else(|| {
        format!(
          "{}: card {}: stock[{}].aspect missing or not a string",
          filename, key, i
        )
      })?;
    let aid = aspect_id(aspect_name)?.ok_or_else(|| {
      format!(
        "{}: card {}: stock[{}].aspect {:?} not declared in aspects.json",
        filename, key, i, aspect_name
      )
    })?;
    if !seen.insert(aid) {
      return Err(format!(
        "{}: card {}: aspect {:?} listed in stock more than once",
        filename, key, aspect_name
      ));
    }

    let max = obj
      .get("max")
      .and_then(Value::as_u64)
      .ok_or_else(|| {
        format!(
          "{}: card {}: stock[{}].max missing or not a non-negative integer",
          filename, key, i
        )
      })? as u64;
    if max == 0 || max > MAX_STOCK_VALUE as u64 {
      return Err(format!(
        "{}: card {}: stock[{}].max {} out of range (1..={})",
        filename, key, i, max, MAX_STOCK_VALUE
      ));
    }
    let max = max as u8;

    // `default` defaults to `max` (an aspect with `max: 3` defaults
    // to 3 stock present — matches the "freshly-generated forest is
    // full" intuition). Authors can pin a lower default explicitly.
    let default = match obj.get("default") {
      None => max,
      Some(v) => {
        let n = v.as_u64().ok_or_else(|| {
          format!(
            "{}: card {}: stock[{}].default not a non-negative integer",
            filename, key, i
          )
        })?;
        if n > max as u64 {
          return Err(format!(
            "{}: card {}: stock[{}].default {} exceeds max {}",
            filename, key, i, n, max
          ));
        }
        n as u8
      }
    };

    // Optional climate-axis coupling. When present, `climate_axis`
    // is one of the four axis names (`elevation` / `temperature` /
    // `humidity` / `aether`); the optional `climate_axis_min` and
    // `climate_axis_max` clamp the input window before quantising
    // to `[0..=max]`. Omitting `climate_axis` means worldgen falls
    // back to an independent FBM band — the v1 default for slots
    // that should speckle (e.g. boulder density on plains)
    // rather than follow a smooth climate gradient.
    let climate_axis = match obj.get("climate_axis") {
      None => None,
      Some(v) => {
        let name = v.as_str().ok_or_else(|| {
          format!(
            "{}: card {}: stock[{}].climate_axis not a string",
            filename, key, i
          )
        })?;
        Some(ClimateAxis::from_name(name).ok_or_else(|| {
          format!(
            "{}: card {}: stock[{}].climate_axis {:?} is not one of \
             elevation / temperature / humidity / aether",
            filename, key, i, name
          )
        })?)
      }
    };

    let climate_axis_min = parse_unit_bound(filename, key, i, obj, "climate_axis_min", 0.0)?;
    let climate_axis_max = parse_unit_bound(filename, key, i, obj, "climate_axis_max", 1.0)?;
    if climate_axis_min >= climate_axis_max {
      return Err(format!(
        "{}: card {}: stock[{}] climate_axis_min {} must be < climate_axis_max {}",
        filename, key, i, climate_axis_min, climate_axis_max
      ));
    }
    if climate_axis.is_none()
      && (obj.contains_key("climate_axis_min") || obj.contains_key("climate_axis_max"))
    {
      return Err(format!(
        "{}: card {}: stock[{}] declares climate_axis_min/_max but no climate_axis",
        filename, key, i
      ));
    }

    out.push(StockSlot {
      aspect_id: aid,
      max,
      default,
      climate_axis,
      climate_axis_min,
      climate_axis_max,
    });
  }
  Ok(out)
}

/// Parse an optional `[0, 1]` float field on a stock-slot object. Uses
/// `default` when absent. Errors out for non-numeric or out-of-range
/// values.
fn parse_unit_bound(
  filename: &str,
  key: &str,
  slot_idx: usize,
  obj: &serde_json::Map<String, Value>,
  field: &str,
  default: f32,
) -> Result<f32, String> {
  match obj.get(field) {
    None => Ok(default),
    Some(v) => {
      let n = v.as_f64().ok_or_else(|| {
        format!(
          "{}: card {}: stock[{}].{} not a number",
          filename, key, slot_idx, field
        )
      })?;
      if !(0.0..=1.0).contains(&n) {
        return Err(format!(
          "{}: card {}: stock[{}].{} {} out of range [0, 1]",
          filename, key, slot_idx, field, n
        ));
      }
      Ok(n as f32)
    }
  }
}

fn parse_aspects(
  filename: &str,
  key: &str,
  aspects_obj: &serde_json::Map<String, Value>,
) -> Result<Vec<(AspectId, i32)>, String> {
  let mut aspects: Vec<(AspectId, i32)> = Vec::with_capacity(aspects_obj.len());
  let mut seen: BTreeSet<AspectId> = BTreeSet::new();
  for (aspect_name, aspect_val) in aspects_obj.iter() {
    let id = aspect_id(aspect_name)?.ok_or_else(|| {
      format!(
        "{}: card {}: unknown aspect {:?} (not declared in aspects.json)",
        filename, key, aspect_name
      )
    })?;
    if !seen.insert(id) {
      return Err(format!(
        "{}: card {}: aspect {:?} listed more than once",
        filename, key, aspect_name
      ));
    }
    let aspect_value = aspect_val.as_i64().ok_or_else(|| {
      format!(
        "{}: card {}: aspect {:?} value not an integer",
        filename, key, aspect_name
      )
    })? as i32;
    aspects.push((id, aspect_value));
  }
  Ok(aspects)
}

/// Parse the optional `lifecycle` block on a card. Also called for
/// the deprecated `magnetic` block (same shape; the caller resolves
/// which key to use).
///
/// Shape:
/// ```json
/// "lifecycle": {
///   "recipe": "<recipe_key>",
///   "duration_ms": 60000
/// }
/// ```
///
/// Returns `(Some(recipe_key), Some(duration_ms))`. Validates duration
/// is a positive integer; recipe-key string is taken verbatim and
/// resolved to a packed recipe id by [`lifecycle_recipe_for_def`] at use
/// time — see the field comment on `CardDefinition.lifecycle_recipe_key`
/// for why resolution is deferred.
fn parse_lifecycle(
  filename: &str,
  key: &str,
  mag_obj: &serde_json::Map<String, Value>,
) -> Result<(Option<String>, Option<u32>), String> {
  let recipe_key = mag_obj
    .get("recipe")
    .ok_or_else(|| {
      format!(
        "{}: card {}: 'lifecycle' missing required 'recipe' field",
        filename, key
      )
    })?
    .as_str()
    .ok_or_else(|| {
      format!(
        "{}: card {}: 'lifecycle.recipe' not a string",
        filename, key
      )
    })?
    .to_string();

  let duration_n = mag_obj
    .get("duration_ms")
    .ok_or_else(|| {
      format!(
        "{}: card {}: 'lifecycle' missing required 'duration_ms' field",
        filename, key
      )
    })?
    .as_u64()
    .ok_or_else(|| {
      format!(
        "{}: card {}: 'lifecycle.duration_ms' not a non-negative integer",
        filename, key
      )
    })?;
  if duration_n == 0 {
    return Err(format!(
      "{}: card {}: 'lifecycle.duration_ms' must be > 0",
      filename, key
    ));
  }
  if duration_n > u32::MAX as u64 {
    return Err(format!(
      "{}: card {}: 'lifecycle.duration_ms' {} exceeds u32 max",
      filename, key, duration_n,
    ));
  }

  Ok((Some(recipe_key), Some(duration_n as u32)))
}

fn parse_traits(
  filename: &str,
  key: &str,
  traits_obj: &serde_json::Map<String, Value>,
) -> Result<Vec<(TraitId, f32)>, String> {
  let mut traits: Vec<(TraitId, f32)> = Vec::with_capacity(traits_obj.len());
  let mut seen: BTreeSet<TraitId> = BTreeSet::new();
  for (trait_name, trait_val) in traits_obj.iter() {
    let id = trait_id(trait_name)?.ok_or_else(|| {
      format!(
        "{}: card {}: unknown trait {:?} (not declared in traits.json)",
        filename, key, trait_name
      )
    })?;
    if !seen.insert(id) {
      return Err(format!(
        "{}: card {}: trait {:?} listed more than once",
        filename, key, trait_name
      ));
    }
    // `as_f64()` accepts both JSON integers and floats — a trait value
    // of `1` parses to `1.0` losslessly, while `1.2` (the `forest_2`
    // tile cost) survives intact. Rejecting non-numeric values keeps
    // the error message specific.
    let trait_value = trait_val.as_f64().ok_or_else(|| {
      format!(
        "{}: card {}: trait {:?} value not a number",
        filename, key, trait_name
      )
    })? as f32;
    traits.push((id, trait_value));
  }
  Ok(traits)
}

fn style_str(filename: &str, key: &str, arr: &[Value], idx: usize) -> Result<String, String> {
  let s = arr[idx]
    .as_str()
    .ok_or_else(|| format!("{}: card {}: style[{}] not a string", filename, key, idx))?;
  if !is_valid_hex_color(s) {
    return Err(format!(
      "{}: card {}: style[{}] {:?} is not a valid #RRGGBB hex color",
      filename, key, idx, s
    ));
  }
  Ok(s.to_string())
}

/// `#RRGGBB` validator. Lowercase or uppercase hex, exactly 6 hex digits.
fn is_valid_hex_color(s: &str) -> bool {
  let bytes = s.as_bytes();
  if bytes.len() != 7 || bytes[0] != b'#' {
    return false;
  }
  bytes[1..].iter().all(|&b| b.is_ascii_hexdigit())
}
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hex_color_validator() {
    assert!(is_valid_hex_color("#000000"));
    assert!(is_valid_hex_color("#FFFFFF"));
    assert!(is_valid_hex_color("#ffffff"));
    assert!(is_valid_hex_color("#a8E0e6"));
    assert!(!is_valid_hex_color("000000"));
    assert!(!is_valid_hex_color("#00000"));
    assert!(!is_valid_hex_color("#0000000"));
    assert!(!is_valid_hex_color("#GGGGGG"));
    assert!(!is_valid_hex_color(""));
    assert!(!is_valid_hex_color("#"));
  }

  /// Authoring check: every card def declaring a `magnetic` block
  /// must point at a recipe of one of the magnetic types. Run via
  /// `bin/content test`. A failure here is a content-authoring bug
  /// — fix the card def's `lifecycle.recipe` field or add the missing
  /// recipe under `recipes/data/magnetic/*.json`.
  #[test]
  fn lifecycle_recipes_resolve() {
    validate_lifecycle_recipes().expect("lifecycle recipe validation");
  }
}
