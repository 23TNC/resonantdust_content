//! Recipe registry built from JSON catalogs in `content/recipes/`.
//!
//! Recipe keys (`"cut_tree"`, etc.) are mapped to stable integer ids in
//! `recipes/id.json`, then packed alongside `recipe_type` (3 bits) and
//! `recipe_category` (3 bits) into a `u16` via [`crate::packed::pack_recipe`].
//! The packed value is what `Action.recipe` carries on the wire.
//!
//! # JSON shape
//!
//! ```jsonc
//! {
//!   "stack": {
//!     "up":   { "<recipe_key>": { ...fields... }, ... },
//!     "down": { ... }
//!   },
//!   "on_create": {
//!     "self":     { ... },
//!     "magnetic": { ... }
//!   },
//!   "magnetic": {
//!     "up":   { ... },
//!     "down": { ... }
//!   }
//! }
//! ```
//!
//! Top-level keys are recipe types (`stack`, `on_create`, `magnetic`).
//! Second-level keys are categories (`up`/`down` for stack & magnetic;
//! `self`/`magnetic` for on_create). Third-level keys ARE the recipe
//! identity — no `"id"` field inside the recipe object. Missing buckets
//! (empty / omitted second-level keys) are valid; treat as `{}`.
//!
//! # Entity grammar
//!
//! An entity is one of:
//!
//! - `"<card_key>"` — bare string, card-key sugar (e.g. `"corpus"`).
//! - `"any"` — sentinel, matches anything (low specificity).
//! - `"@<type_name>"` — sentinel, matches any card whose `card_type`
//!   resolves to the named type via `cards/types.json`.
//! - `["<entity>", "<entity>", ...]` — OR-list sugar; matches if any
//!   element matches. Recursive.
//! - `{"card":     "<key>"}` — explicit card key (no advantage over the
//!   bare string today; reserved for future cases where the key shadows
//!   a sentinel).
//! - `{"aspect":   "<name>", "min": N}` — match any card whose aspect
//!   value is ≥ N.
//! - `{"category": "<name>"}` — match by card category.
//! - `{"flag":     "<name>"}` — match cards carrying this flag bit.
//! - `{"any":      true}` — explicit wildcard.
//! - `{"and":      [entity, ...]}` — conjunction.
//! - `{"or":       [entity, ...]}` — disjunction (same as bare-array
//!   sugar; provided for explicitness inside tagged trees).
//! - `{"not":      <entity>}` — negation.
//!
//! Tagged objects are dispatched by the first recognized key. The
//! grammar is open — new predicate keys land here as new [`Entity`]
//! variants + new parse arms.
//!
//! # Other field shapes
//!
//! - **`reagents`**: `{ "slots": [<u8>...], "roles": [<string>...],
//!   "has"?: {...}, "has_below"?: {...} }`. Slot indices are 0-indexed
//!   (slot 0 = actor). Role names are `"root"` and `"hex"`. All four
//!   keys optional; absent reagents = nothing consumed.
//! - **`duration`**: either a bare number (constant seconds) or an
//!   array of tier objects `[{ "when"?: <entity>, "seconds": <u32> },
//!   ...]`. First matching `when` wins, in declaration order. A tier
//!   without `when` is the unconditional fallback.
//! - **`set`**: `{ "start": { "<role>": { "<flag>": <bool>, ... } } }`.
//!   Roles are `root`, `slot`, `hex`. `true` = force-on, `false` =
//!   force-off, omitted = untouched.
//! - **`style`**: string enum mapped to the u3 `progress_style` field
//!   on `Card.flags`. Supported values: `"none"` (0), `"ltr"` (1),
//!   `"rtl"` (2). Future styles slot into 3..=7. Omitting the field
//!   reads as `"none"`.
//! - **`output`** / **`output_failure`**: `{ "<place>": { "<owner>":
//!   [<card_key>, ...] } }`. Place is `inventory` or `location`. Owner
//!   is `root`, `actor`, `hex`, or `action`. Spawn lists are flat
//!   card-key arrays (not predicates — they're constructors).
//!   `output_failure` only applies to magnetic outers (loop cap hit
//!   without dispatching to an inner).
//! - **`magnetic`** (on `on_create.magnetic` recipes): `{ "success":
//!   "magnetic.<dir>.<key>", "failure": "magnetic.<dir>.<key>" }`.
//!   Dotted paths reference inner recipes filed under
//!   `magnetic.up.*` / `magnetic.down.*`; the leading
//!   `magnetic.<dir>.` is necessary because recipe keys can collide
//!   across direction buckets.
//! - **`hex`** / **`root`**: single entity (or OR-list-sugar array).
//! - **`slots`**: array of slots, each slot is an entity.
//! - **`has`** / **`has_below`**: `{ "root"?: [entity, ...], "actor"?:
//!   [entity, ...] }` — each entry is a slot on the named soul-stack.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::definition_core::{aspect_id, card_type_ids, decode_definition, AspectId, CardDefinition};
use crate::flags_core::card_flag_bit;
use crate::packed::{pack_recipe, RECIPE_ID_MASK, RECIPE_TYPE_OR_CATEGORY_MASK};

// ---------- Entity ----------

/// A predicate against a card. Used to validate slot fillers (the tree
/// is matched against a candidate card and returns a specificity score)
/// and to drive product generation (where `WeightedOr` selects between
/// outputs). See module doc for the JSON grammar.
///
/// # Match specificity (used by the priority weighting in `actions.rs`)
///
/// Leaf weights:
/// - `Card`:     4
/// - `Aspect`:   3
/// - `Category`: 3
/// - `Type`:     2
/// - `Flag`:     2
/// - `Any`:      1
///
/// `And` requires every child to match; specificity is the sum of
/// children's weights. `Or` / `WeightedOr` take the max of children
/// that satisfied. `Not` inverts — specificity 1 when the child does
/// NOT match (a permissive "anything but X"), 0 when it does.
#[derive(Debug, Clone)]
pub enum Entity {
  Card(String),
  /// Match any card with `aspects[aspect] >= min`.
  Aspect(AspectId, i32),
  /// Match any card whose `card_type` equals this id.
  Type(u8),
  /// Match any card whose `card_category` equals this id.
  Category(u8),
  /// Match any card carrying the named flag (bit position).
  Flag(u8),
  /// Match any card. Lowest specificity.
  Any,
  /// Every child must match. N-ary so `{"and": [a, b, c]}` is one
  /// flat conjunction instead of right-associated pairs.
  And(Vec<Entity>),
  /// Any child must match. N-ary; matches the JSON array shape
  /// directly.
  Or(Vec<Entity>),
  /// Child must NOT match. Specificity 1 when satisfied, 0 otherwise.
  Not(Box<Entity>),
  /// Binary, weighted OR for product selection at completion. Weights
  /// steer random choice between the two branches; match scoring
  /// treats it as a plain `Or` (max of the two).
  WeightedOr {
    a: Box<Entity>,
    b: Box<Entity>,
    weight_a: u32,
    weight_b: u32,
  },
}

// ---------- RecipeType / direction ----------

/// What shape of trigger fires the recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeType {
  /// `stack.up` / `stack.down` — fired by `propose_action` when a
  /// player submits a chain that the matcher fits a recipe slot
  /// window against.
  Stack(StackDirection),
  /// `on_create.self` — fired when a card is inserted; the new card
  /// is both root and actor of the resulting action.
  OnCreate,
  /// `on_create.magnetic` — fired on insert, installs a magnetic
  /// ticker on the new card. The outer recipe carries `magnetic:
  /// { success, failure }` referencing inner recipes filed under
  /// `magnetic.{up,down}.<key>` which the ticker dispatches between.
  OnCreateMagnetic,
  /// `magnetic.up` / `magnetic.down` — inner recipes a magnetic
  /// ticker dispatches to. Lookup-only (never matched against a
  /// chain directly); referenced by dotted path from an
  /// `OnCreateMagnetic` outer's `magnetic.success` / `magnetic.failure`
  /// fields.
  Magnetic(StackDirection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
  Up,
  Down,
}

/// Resolved stable ids of the two inner recipes a magnetic outer
/// dispatches between. Parsed from the outer's `magnetic.success` /
/// `magnetic.failure` dotted-path strings; resolved to ids in a second
/// pass after every recipe has been registered.
#[derive(Debug, Clone, Copy)]
pub struct MagneticRefs {
  pub success: u16,
  pub failure: u16,
}

// ---------- Product destinations ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductTarget {
  pub place: ProductPlace,
  pub owner: ProductOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductPlace {
  Inventory,
  Location,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductOwner {
  Root,
  Actor,
  Hex,
  Action,
}

#[derive(Debug, Clone)]
pub struct ProductGroup {
  pub target: ProductTarget,
  pub entities: Vec<Entity>,
}

// ---------- Set ops ----------

/// Bits to set / clear on a card's `flags` u32. `true` in the JSON
/// lands in `set_mask` (force-on); `false` lands in `clear_mask`
/// (force-off). Apply via [`FlagOps::apply`].
#[derive(Debug, Clone, Copy, Default)]
pub struct FlagOps {
  pub set_mask: u32,
  pub clear_mask: u32,
}

impl FlagOps {
  /// Clear bits first, then set bits, so an author's `true` always
  /// wins over their own `false` if both somehow named the same flag
  /// (which the parser doesn't allow per-key).
  pub fn apply(self, flags: u32) -> u32 {
    (flags & !self.clear_mask) | self.set_mask
  }
}

/// Flag deltas to apply at action start, scoped per role. JSON form:
/// `set: { start: { root: {...}, slot: {...}, hex: {...} } }`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SetStartFlags {
  pub root: FlagOps,
  pub slot: FlagOps,
  pub hex: FlagOps,
}

// ---------- Reagents ----------

/// What a recipe consumes on completion.
///
/// - **Slots** — 0-indexed slot positions. `Slot(0)` is the actor;
///   higher indices are non-actor slot fillers.
/// - **Roles** — named referents. `Root` consumes the chain root;
///   `Hex` consumes the hex card the chain is anchored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reagent {
  Root,
  Hex,
  Slot(u8),
}

/// Per-direction has-predicate lists. Each entry is one slot that
/// must be filled by a card in the relevant soul-stack.
#[derive(Debug, Clone, Default)]
pub struct HasOps {
  pub root: Vec<Entity>,
  pub actor: Vec<Entity>,
}

/// Full reagent spec — what dies at completion. All four fields
/// optional.
#[derive(Debug, Clone, Default)]
pub struct Reagents {
  pub slots: Vec<Reagent>,
  pub has: HasOps,
  pub has_below: HasOps,
}

// ---------- Duration ----------

/// Recipe duration. Either fixed seconds, or a tier list with an
/// unconditional fallback at the tail.
#[derive(Debug, Clone)]
pub enum Duration {
  Fixed(u32),
  Conditional {
    /// First matching `when` wins.
    cases: Vec<(u32, Entity)>,
    /// Used when no `when` matches.
    fallback: u32,
  },
}

// ---------- Recipe definition ----------

#[derive(Debug, Clone)]
pub struct RecipeDef {
  /// Packed stable id (see `pack_recipe`).
  pub index: u16,
  /// Human-readable id — the recipe's key in the JSON tree.
  pub id: String,
  pub recipe_type: RecipeType,
  pub root: Option<Entity>,
  pub hex: Option<Entity>,
  pub slots: Vec<Entity>,
  pub reagents: Reagents,
  pub has: HasOps,
  pub has_below: HasOps,
  /// Cards produced on success. JSON key: `output`. Empty when the
  /// recipe has no spawn step.
  pub output: Vec<ProductGroup>,
  /// Cards produced when a magnetic outer's loop cap is reached
  /// without dispatching. JSON key: `output_failure`. Empty for
  /// non-magnetic recipes (parser rejects).
  pub output_failure: Vec<ProductGroup>,
  pub duration: Option<Duration>,
  /// Set only on `OnCreateMagnetic` recipes. Resolved in the second
  /// build pass after every recipe id is known.
  pub magnetic: Option<MagneticRefs>,
  pub interval: Option<u32>,
  pub delay: Option<u32>,
  pub set_start: SetStartFlags,
  /// `progress_style` field value (0..=7); see `cards/flags.json`.
  pub style: u8,
}

// ---------- Registries ----------

use crate::embedded_data::RECIPES_FILES;

const RECIPE_IDS_JSON: &str = include_str!("../recipes/id.json");
const RECIPE_TYPES_JSON: &str = include_str!("../recipes/types.json");

struct RecipeRegistry {
  by_id: BTreeMap<u16, RecipeDef>,
  id_by_name: BTreeMap<String, u16>,
  by_type: BTreeMap<(u8, u8), Vec<u16>>,
}

static RECIPES: OnceLock<Result<RecipeRegistry, String>> = OnceLock::new();

fn recipes_registry() -> Result<&'static RecipeRegistry, String> {
  RECIPES.get_or_init(build_recipes).as_ref().map_err(|e| e.clone())
}

pub fn recipe(index: u16) -> Result<Option<&'static RecipeDef>, String> {
  Ok(recipes_registry()?.by_id.get(&index))
}

pub fn find_recipe(id: &str) -> Result<Option<&'static RecipeDef>, String> {
  let registry = recipes_registry()?;
  let Some(&stable_id) = registry.id_by_name.get(id) else {
    return Ok(None);
  };
  Ok(registry.by_id.get(&stable_id))
}

pub fn recipes_of_type(rt: RecipeType) -> Result<Vec<&'static RecipeDef>, String> {
  let registry = recipes_registry()?;
  let key = recipe_type_pair(rt)?;
  let Some(ids) = registry.by_type.get(&key) else {
    return Ok(Vec::new());
  };
  Ok(ids.iter().filter_map(|id| registry.by_id.get(id)).collect())
}

// ---------- Stack matching ----------

#[derive(Debug, Clone)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(feature = "js", serde(rename_all = "camelCase"))]
pub struct StackMatch {
  pub recipe_index: u16,
  pub slot_start: u32,
  pub slot_count: u32,
  pub has_root: bool,
  pub has_hex: bool,
}

pub fn match_stack_recipe(
  hex_def: u16,
  root_def: u16,
  slot_defs: &[u16],
  direction: StackDirection,
) -> Result<u16, String> {
  Ok(
    match_stack_recipe_detail(hex_def, root_def, slot_defs, direction, None)?
      .map(|m| m.recipe_index)
      .unwrap_or(0),
  )
}

#[derive(Debug, Default)]
pub struct HasCandidates<'a> {
  pub root_above: Vec<&'a CardDefinition>,
  pub actor_above: Vec<&'a CardDefinition>,
  pub root_below: Vec<&'a CardDefinition>,
  pub actor_below: Vec<&'a CardDefinition>,
}

pub fn has_predicates_feasible(
  recipe: &RecipeDef,
  candidates: &HasCandidates,
) -> bool {
  role_all_entries_feasible(
    recipe.has.root.iter().chain(recipe.reagents.has.root.iter()),
    &candidates.root_above,
  ) && role_all_entries_feasible(
    recipe.has.actor.iter().chain(recipe.reagents.has.actor.iter()),
    &candidates.actor_above,
  ) && role_all_entries_feasible(
    recipe
      .has_below
      .root
      .iter()
      .chain(recipe.reagents.has_below.root.iter()),
    &candidates.root_below,
  ) && role_all_entries_feasible(
    recipe
      .has_below
      .actor
      .iter()
      .chain(recipe.reagents.has_below.actor.iter()),
    &candidates.actor_below,
  )
}

fn role_all_entries_feasible<'a, I>(entries: I, pool: &[&CardDefinition]) -> bool
where
  I: IntoIterator<Item = &'a Entity>,
{
  for entry in entries {
    let any_match = pool.iter().any(|d| entity_specificity(entry, d) > 0);
    if !any_match {
      return false;
    }
  }
  true
}

pub fn match_stack_recipe_detail(
  hex_def: u16,
  root_def: u16,
  slot_defs: &[u16],
  direction: StackDirection,
  has_candidates: Option<&HasCandidates>,
) -> Result<Option<StackMatch>, String> {
  let candidates = recipes_of_type(RecipeType::Stack(direction))?;

  let hex_card = decode_definition(hex_def)?;
  let root_card = decode_definition(root_def)?;
  let slot_cards: Vec<Option<&CardDefinition>> = slot_defs
    .iter()
    .map(|&d| decode_definition(d))
    .collect::<Result<_, _>>()?;

  let chain: Vec<Option<&CardDefinition>> = std::iter::once(root_card)
    .chain(slot_cards.iter().copied())
    .collect();

  let mut best: Option<((u32, u32, u32), StackMatch)> = None;

  'recipes: for recipe in candidates {
    let hex_spec = match &recipe.hex {
      None => 0,
      Some(e) => {
        let Some(def) = hex_card else { continue 'recipes };
        let s = entity_specificity(e, def);
        if s == 0 {
          continue 'recipes;
        }
        s
      }
    };

    let root_spec = match &recipe.root {
      None => 0,
      Some(e) => {
        let Some(def) = root_card else { continue 'recipes };
        let s = entity_specificity(e, def);
        if s == 0 {
          continue 'recipes;
        }
        s
      }
    };

    if let Some(hc) = has_candidates {
      if !has_predicates_feasible(recipe, hc) {
        continue 'recipes;
      }
    }

    let min_start: usize = if recipe.root.is_some() { 1 } else { 0 };
    if chain.len() < min_start + recipe.slots.len() {
      continue 'recipes;
    }
    let max_start: usize = chain.len() - recipe.slots.len();

    let mut best_for_recipe: Option<(u32, u32)> = None;
    for start in min_start..=max_start {
      let mut spec_sum: u32 = 0;
      let mut ok = true;
      for (i, slot_entity) in recipe.slots.iter().enumerate() {
        let Some(def) = chain[start + i] else {
          ok = false;
          break;
        };
        let s = entity_specificity(slot_entity, def);
        if s == 0 {
          ok = false;
          break;
        }
        spec_sum += s;
      }
      if ok {
        best_for_recipe = Some(match best_for_recipe {
          None => (spec_sum, start as u32),
          Some((b, _)) if spec_sum > b => (spec_sum, start as u32),
          Some(prev) => prev,
        });
      }
    }

    let Some((slot_spec, slot_start)) = best_for_recipe else {
      continue 'recipes;
    };

    let score = (hex_spec, root_spec, slot_spec);
    if best.as_ref().map_or(true, |(b, _)| score > *b) {
      best = Some((
        score,
        StackMatch {
          recipe_index: recipe.index,
          slot_start,
          slot_count: recipe.slots.len() as u32,
          has_root: recipe.root.is_some(),
          has_hex: recipe.hex.is_some(),
        },
      ));
    }
  }

  Ok(best.map(|(_, m)| m))
}

/// Score how well a single entity matches a card definition; 0 means
/// no match. See [`Entity`] for weight conventions.
pub fn entity_specificity(entity: &Entity, def: &CardDefinition) -> u32 {
  match entity {
    Entity::Card(key) => {
      if &def.key == key { 4 } else { 0 }
    }
    Entity::Aspect(aspect, min) => {
      let val = def
        .aspects
        .iter()
        .find_map(|(a, v)| (a == aspect).then_some(*v))
        .unwrap_or(0);
      if val >= *min { 3 } else { 0 }
    }
    Entity::Type(type_id) => {
      if def.card_type == *type_id { 2 } else { 0 }
    }
    Entity::Category(cat_id) => {
      if def.card_category == *cat_id { 3 } else { 0 }
    }
    Entity::Flag(bit) => {
      if def.flags & (1u32 << bit) != 0 { 2 } else { 0 }
    }
    Entity::Any => 1,
    Entity::And(children) => {
      let mut sum: u32 = 0;
      for c in children {
        let s = entity_specificity(c, def);
        if s == 0 {
          return 0;
        }
        sum = sum.saturating_add(s);
      }
      sum
    }
    Entity::Or(children) => children
      .iter()
      .map(|c| entity_specificity(c, def))
      .max()
      .unwrap_or(0),
    Entity::Not(child) => {
      if entity_specificity(child, def) == 0 { 1 } else { 0 }
    }
    Entity::WeightedOr { a, b, .. } => {
      entity_specificity(a, def).max(entity_specificity(b, def))
    }
  }
}

// ---------- Recipe types registry (recipes/types.json) ----------

struct RecipeTypeRegistry {
  types: BTreeMap<String, u8>,
  categories: BTreeMap<String, u8>,
}

static RECIPE_TYPES: OnceLock<Result<RecipeTypeRegistry, String>> = OnceLock::new();

fn recipe_types_registry() -> Result<&'static RecipeTypeRegistry, String> {
  RECIPE_TYPES.get_or_init(build_recipe_types).as_ref().map_err(|e| e.clone())
}

fn build_recipe_types() -> Result<RecipeTypeRegistry, String> {
  let root: Value = serde_json::from_str(RECIPE_TYPES_JSON)
    .map_err(|e| format!("recipes/types.json: parse failed: {}", e))?;
  let types = recipe_id_section(&root, "types")?;
  let categories = recipe_id_section(&root, "categories")?;
  Ok(RecipeTypeRegistry { types, categories })
}

fn recipe_id_section(root: &Value, section: &str) -> Result<BTreeMap<String, u8>, String> {
  let section_obj = root
    .get(section)
    .and_then(Value::as_object)
    .ok_or_else(|| format!("recipes/types.json: '{}' missing or not an object", section))?;
  let mut result = BTreeMap::new();
  for (name, info) in section_obj {
    if name.starts_with('_') {
      continue;
    }
    let id_value = info.get("id").ok_or_else(|| {
      format!("recipes/types.json: '{}' entry {:?} missing 'id'", section, name)
    })?;
    let id_u64 = id_value.as_u64().ok_or_else(|| {
      format!(
        "recipes/types.json: '{}' entry {:?} 'id' not a non-negative integer",
        section, name
      )
    })?;
    if id_u64 > RECIPE_TYPE_OR_CATEGORY_MASK as u64 {
      return Err(format!(
        "recipes/types.json: '{}' entry {:?} id {} exceeds u3 max ({})",
        section, name, id_u64, RECIPE_TYPE_OR_CATEGORY_MASK,
      ));
    }
    result.insert(name.clone(), id_u64 as u8);
  }
  Ok(result)
}

fn recipe_type_pair(rt: RecipeType) -> Result<(u8, u8), String> {
  let registry = recipe_types_registry()?;
  let (type_name, category_name) = recipe_type_names(rt);
  let &type_id = registry.types.get(type_name).ok_or_else(|| {
    format!("recipes/types.json: type {:?} missing", type_name)
  })?;
  let &category_id = registry.categories.get(category_name).ok_or_else(|| {
    format!("recipes/types.json: category {:?} missing", category_name)
  })?;
  Ok((type_id, category_id))
}

fn recipe_type_names(rt: RecipeType) -> (&'static str, &'static str) {
  match rt {
    RecipeType::Stack(StackDirection::Up) => ("stack", "up"),
    RecipeType::Stack(StackDirection::Down) => ("stack", "down"),
    RecipeType::OnCreate => ("on_create", "self"),
    RecipeType::OnCreateMagnetic => ("on_create", "magnetic"),
    RecipeType::Magnetic(StackDirection::Up) => ("magnetic", "up"),
    RecipeType::Magnetic(StackDirection::Down) => ("magnetic", "down"),
  }
}

/// Map a `(type_name, category_name)` pair from a data file's nested
/// path back to a `RecipeType`. `None` for unknown combinations
/// (loader skips those — silent so future bucket types can ship
/// before the Rust enum learns about them).
fn parse_recipe_type(type_name: &str, category_name: &str) -> Option<RecipeType> {
  match (type_name, category_name) {
    ("stack", "up") => Some(RecipeType::Stack(StackDirection::Up)),
    ("stack", "down") => Some(RecipeType::Stack(StackDirection::Down)),
    ("on_create", "self") => Some(RecipeType::OnCreate),
    ("on_create", "magnetic") => Some(RecipeType::OnCreateMagnetic),
    ("magnetic", "up") => Some(RecipeType::Magnetic(StackDirection::Up)),
    ("magnetic", "down") => Some(RecipeType::Magnetic(StackDirection::Down)),
    _ => None,
  }
}

// ---------- Main loader ----------

fn build_recipes() -> Result<RecipeRegistry, String> {
  let type_registry = recipe_types_registry()?;

  // recipes/id.json: { "<type>": { "<category>": { "<key>": <id> } } }
  let ids_root: Value = serde_json::from_str(RECIPE_IDS_JSON)
    .map_err(|e| format!("recipes/id.json: parse failed: {}", e))?;
  let ids_obj = ids_root
    .as_object()
    .ok_or_else(|| "recipes/id.json: top-level not an object".to_string())?;

  let mut packed_ids: BTreeMap<String, u16> = BTreeMap::new();
  for (type_name, type_val) in ids_obj {
    let &type_id = type_registry.types.get(type_name).ok_or_else(|| {
      format!("recipes/id.json: type {:?} not in recipes/types.json", type_name)
    })?;
    let type_obj = type_val.as_object().ok_or_else(|| {
      format!("recipes/id.json: entry for type {:?} not an object", type_name)
    })?;
    for (category_name, cat_val) in type_obj {
      let &category_id = type_registry.categories.get(category_name).ok_or_else(|| {
        format!(
          "recipes/id.json: category {:?} under type {:?} not in recipes/types.json",
          category_name, type_name
        )
      })?;
      let cat_obj = cat_val.as_object().ok_or_else(|| {
        format!(
          "recipes/id.json: entry for {:?}/{:?} not an object",
          type_name, category_name
        )
      })?;
      for (key, val) in cat_obj {
        let n = val.as_u64().ok_or_else(|| {
          format!(
            "recipes/id.json: id for {:?}/{:?}/{:?} not an integer",
            type_name, category_name, key
          )
        })?;
        if n == 0 || n > RECIPE_ID_MASK as u64 {
          return Err(format!(
            "recipes/id.json: id {} for {:?}/{:?}/{:?} out of range (1..={})",
            n, type_name, category_name, key, RECIPE_ID_MASK,
          ));
        }
        let packed = pack_recipe(type_id, category_id, n as u16);
        if let Some(prev) = packed_ids.insert(key.clone(), packed) {
          return Err(format!(
            "recipes/id.json: recipe key {:?} declared more than once (prev packed={:#06x}, new={:#06x})",
            key, prev, packed,
          ));
        }
      }
    }
  }

  let type_ids = card_type_ids()?.clone();

  let mut by_id: BTreeMap<u16, RecipeDef> = BTreeMap::new();
  let mut id_by_name: BTreeMap<String, u16> = BTreeMap::new();
  let mut by_type: BTreeMap<(u8, u8), Vec<u16>> = BTreeMap::new();
  // (outer_packed_id, success_path, failure_path, filename) — resolved
  // in second pass below.
  let mut pending_magnetic: Vec<(u16, String, String, &'static str)> = Vec::new();

  for (filename, content) in RECIPES_FILES {
    let parsed: Value = serde_json::from_str(content)
      .map_err(|e| format!("{}: parse failed: {}", filename, e))?;
    let root = parsed.as_object().ok_or_else(|| {
      format!("{}: top-level must be an object keyed by recipe type", filename)
    })?;

    for (type_name, by_category_val) in root {
      let categories = by_category_val.as_object().ok_or_else(|| {
        format!("{}: type {:?}: value not an object", filename, type_name)
      })?;

      for (category_name, by_key_val) in categories {
        // Unknown (type, category) pairs are silently skipped — same
        // discipline as cards loader. Lets content files outpace the
        // Rust enum.
        let Some(recipe_type) = parse_recipe_type(type_name, category_name) else {
          continue;
        };
        let pair = recipe_type_pair(recipe_type)?;
        let recipes_obj = by_key_val.as_object().ok_or_else(|| {
          format!(
            "{}: {}/{}: value not an object of recipe keys",
            filename, type_name, category_name
          )
        })?;

        for (key, recipe_value) in recipes_obj {
          let stable_id = *packed_ids.get(key).ok_or_else(|| {
            format!(
              "{}: recipe {:?} ({}/{}) not in recipes/id.json — run gen-ids.py",
              filename, key, type_name, category_name
            )
          })?;
          if id_by_name.contains_key(key) {
            return Err(format!(
              "{}: recipe key {:?} declared more than once",
              filename, key
            ));
          }
          let (def, pending) =
            parse_recipe(key, recipe_value, recipe_type, stable_id, filename, &type_ids)?;
          by_type.entry(pair).or_default().push(stable_id);
          id_by_name.insert(key.clone(), stable_id);
          by_id.insert(stable_id, def);
          if let Some((s, f)) = pending {
            pending_magnetic.push((stable_id, s, f, filename));
          }
        }
      }
    }
  }

  // Second pass: resolve magnetic outer refs.
  for (outer_id, succ_path, fail_path, filename) in pending_magnetic {
    let success = resolve_magnetic_path(&succ_path, "success", outer_id, filename, &id_by_name, &by_id)?;
    let failure = resolve_magnetic_path(&fail_path, "failure", outer_id, filename, &id_by_name, &by_id)?;
    if let Some(def) = by_id.get_mut(&outer_id) {
      def.magnetic = Some(MagneticRefs { success, failure });
    }
  }

  Ok(RecipeRegistry { by_id, id_by_name, by_type })
}

fn resolve_magnetic_path(
  path: &str,
  field: &str,
  outer_id: u16,
  filename: &str,
  id_by_name: &BTreeMap<String, u16>,
  by_id: &BTreeMap<u16, RecipeDef>,
) -> Result<u16, String> {
  let parts: Vec<&str> = path.split('.').collect();
  if parts.len() != 3 {
    return Err(format!(
      "{}: magnetic outer (stable_id={}) magnetic.{} {:?} not a 'type.category.id' path",
      filename, outer_id, field, path
    ));
  }
  let (path_type, path_category, leaf) = (parts[0], parts[1], parts[2]);
  let resolved = *id_by_name.get(leaf).ok_or_else(|| {
    format!(
      "{}: magnetic outer (stable_id={}) magnetic.{} {:?}: leaf id {:?} not found",
      filename, outer_id, field, path, leaf
    )
  })?;
  let def = by_id.get(&resolved).expect("id_by_name and by_id share keys");
  if !matches!(def.recipe_type, RecipeType::Magnetic(_)) {
    return Err(format!(
      "{}: magnetic outer (stable_id={}) magnetic.{} {:?} resolved to non-magnetic recipe (got {:?})",
      filename, outer_id, field, path, def.recipe_type
    ));
  }
  let (actual_type, actual_category) = recipe_type_names(def.recipe_type);
  if path_type != actual_type || path_category != actual_category {
    return Err(format!(
      "{}: magnetic outer (stable_id={}) magnetic.{} {:?} prefix mismatch — leaf {:?} is filed under {:?}.{:?}",
      filename, outer_id, field, path, leaf, actual_type, actual_category
    ));
  }
  Ok(resolved)
}

// ---------- Per-recipe parsing ----------

fn parse_recipe(
  key: &str,
  recipe_value: &Value,
  recipe_type: RecipeType,
  stable_id: u16,
  filename: &str,
  type_ids: &BTreeMap<String, u8>,
) -> Result<(RecipeDef, Option<(String, String)>), String> {
  let obj = recipe_value.as_object().ok_or_else(|| {
    format!("{}: recipe {:?}: value not an object", filename, key)
  })?;

  let root = obj
    .get("root")
    .map(|v| parse_entity(v, type_ids, filename, key, "root"))
    .transpose()?;

  let hex = obj
    .get("hex")
    .map(|v| parse_entity(v, type_ids, filename, key, "hex"))
    .transpose()?;

  let slots = match obj.get("slots") {
    None => Vec::new(),
    Some(Value::Array(arr)) => arr
      .iter()
      .enumerate()
      .map(|(i, v)| parse_entity(v, type_ids, filename, key, &format!("slots[{}]", i)))
      .collect::<Result<Vec<_>, _>>()?,
    Some(_) => {
      return Err(format!(
        "{}: recipe {:?}: 'slots' must be an array of entities",
        filename, key
      ));
    }
  };

  let reagents = parse_reagents(obj.get("reagents"), type_ids, filename, key)?;
  let has = parse_has_ops(obj.get("has"), type_ids, filename, key, "has")?;
  let has_below = parse_has_ops(obj.get("has_below"), type_ids, filename, key, "has_below")?;

  let output = parse_output_groups(obj.get("output"), type_ids, filename, key, "output")?;
  let output_failure =
    parse_output_groups(obj.get("output_failure"), type_ids, filename, key, "output_failure")?;
  if !output_failure.is_empty() && recipe_type != RecipeType::OnCreateMagnetic {
    return Err(format!(
      "{}: recipe {:?}: 'output_failure' only valid on on_create.magnetic outers",
      filename, key
    ));
  }

  let duration = obj
    .get("duration")
    .map(|v| parse_duration(v, type_ids, filename, key))
    .transpose()?;
  if duration.is_none() && recipe_type != RecipeType::OnCreateMagnetic {
    return Err(format!(
      "{}: recipe {:?}: missing required 'duration' (only magnetic outers may omit it)",
      filename, key
    ));
  }

  if matches!(recipe_type, RecipeType::OnCreate | RecipeType::OnCreateMagnetic)
    && root.is_none()
    && hex.is_none()
  {
    return Err(format!(
      "{}: recipe {:?}: on_create recipes must specify 'root' or 'hex'",
      filename, key
    ));
  }

  let interval = parse_u32_field(obj.get("interval"), filename, key, "interval")?;
  let delay = parse_u32_field(obj.get("delay"), filename, key, "delay")?;
  if recipe_type != RecipeType::OnCreateMagnetic {
    if interval.is_some() {
      return Err(format!(
        "{}: recipe {:?}: 'interval' only valid on on_create.magnetic outers",
        filename, key
      ));
    }
    if delay.is_some() {
      return Err(format!(
        "{}: recipe {:?}: 'delay' only valid on on_create.magnetic outers",
        filename, key
      ));
    }
  }

  let pending_magnetic = match obj.get("magnetic") {
    None => None,
    Some(v) => {
      if recipe_type != RecipeType::OnCreateMagnetic {
        return Err(format!(
          "{}: recipe {:?}: 'magnetic' refs only valid on on_create.magnetic outers",
          filename, key
        ));
      }
      let m = v.as_object().ok_or_else(|| {
        format!(
          "{}: recipe {:?}: 'magnetic' must be an object with 'success' and 'failure' keys",
          filename, key
        )
      })?;
      let succ = m
        .get("success")
        .and_then(Value::as_str)
        .ok_or_else(|| {
          format!("{}: recipe {:?}: 'magnetic.success' missing or not a string", filename, key)
        })?
        .to_string();
      let fail = m
        .get("failure")
        .and_then(Value::as_str)
        .ok_or_else(|| {
          format!("{}: recipe {:?}: 'magnetic.failure' missing or not a string", filename, key)
        })?
        .to_string();
      Some((succ, fail))
    }
  };
  if recipe_type == RecipeType::OnCreateMagnetic && pending_magnetic.is_none() {
    return Err(format!(
      "{}: recipe {:?}: on_create.magnetic outer must define 'magnetic.success' and 'magnetic.failure'",
      filename, key
    ));
  }

  let set_start = parse_set(obj.get("set"), filename, key)?;
  let style = parse_style(obj.get("style"), filename, key)?;

  let def = RecipeDef {
    index: stable_id,
    id: key.to_string(),
    recipe_type,
    root,
    hex,
    slots,
    reagents,
    has,
    has_below,
    output,
    output_failure,
    duration,
    magnetic: None,
    interval,
    delay,
    set_start,
    style,
  };
  Ok((def, pending_magnetic))
}

fn parse_u32_field(
  value: Option<&Value>,
  filename: &str,
  recipe_id: &str,
  field: &str,
) -> Result<Option<u32>, String> {
  let Some(v) = value else { return Ok(None) };
  let n = v.as_u64().ok_or_else(|| {
    format!("{}: recipe {:?}: {:?} not a non-negative integer: {:?}", filename, recipe_id, field, v)
  })?;
  u32::try_from(n).map(Some).map_err(|_| {
    format!("{}: recipe {:?}: {} {} exceeds u32 range", filename, recipe_id, field, n)
  })
}

// ---------- Style ----------

fn parse_style(value: Option<&Value>, filename: &str, recipe_id: &str) -> Result<u8, String> {
  let Some(v) = value else { return Ok(0) };
  let s = v.as_str().ok_or_else(|| {
    format!(
      "{}: recipe {:?}: 'style' must be a string (\"none\", \"ltr\", \"rtl\", ...); got {:?}",
      filename, recipe_id, v
    )
  })?;
  match s {
    "none" => Ok(0),
    "ltr" => Ok(1),
    "rtl" => Ok(2),
    other => Err(format!(
      "{}: recipe {:?}: unknown style {:?} (known: \"none\", \"ltr\", \"rtl\")",
      filename, recipe_id, other
    )),
  }
}

// ---------- Set / SetStartFlags ----------

fn parse_set(
  value: Option<&Value>,
  filename: &str,
  recipe_id: &str,
) -> Result<SetStartFlags, String> {
  let Some(v) = value else { return Ok(SetStartFlags::default()) };
  let obj = v.as_object().ok_or_else(|| {
    format!("{}: recipe {:?}: 'set' must be an object", filename, recipe_id)
  })?;
  // Only `start` is defined today; other timing keys (`end`, …) reserved.
  for k in obj.keys() {
    if k != "start" {
      return Err(format!(
        "{}: recipe {:?}: set.{} unknown timing key (known: \"start\")",
        filename, recipe_id, k
      ));
    }
  }
  let Some(start_val) = obj.get("start") else {
    return Ok(SetStartFlags::default());
  };
  let start_obj = start_val.as_object().ok_or_else(|| {
    format!("{}: recipe {:?}: 'set.start' must be an object", filename, recipe_id)
  })?;
  let mut flags = SetStartFlags::default();
  for (role, flags_val) in start_obj {
    let flags_obj = flags_val.as_object().ok_or_else(|| {
      format!(
        "{}: recipe {:?}: set.start.{} must be an object of flag→bool entries",
        filename, recipe_id, role
      )
    })?;
    let mut ops = FlagOps::default();
    for (flag_name, bit_val) in flags_obj {
      let set = bit_val.as_bool().ok_or_else(|| {
        format!(
          "{}: recipe {:?}: set.start.{}.{} not a boolean",
          filename, recipe_id, role, flag_name
        )
      })?;
      if flag_name == "dead" {
        return Err(format!(
          "{}: recipe {:?}: set.start.{}.dead: 'dead' cannot be set via set.start",
          filename, recipe_id, role
        ));
      }
      let bit = card_flag_bit(flag_name)
        .map_err(|e| format!("{}: recipe {:?}: flag registry: {}", filename, recipe_id, e))?
        .ok_or_else(|| {
          format!(
            "{}: recipe {:?}: set.start.{}.{}: unknown flag (not in cards/flags.json)",
            filename, recipe_id, role, flag_name
          )
        })?;
      if set {
        ops.set_mask |= 1u32 << bit;
      } else {
        ops.clear_mask |= 1u32 << bit;
      }
    }
    match role.as_str() {
      "root" => flags.root = ops,
      "slot" => flags.slot = ops,
      "hex" => flags.hex = ops,
      other => {
        return Err(format!(
          "{}: recipe {:?}: set.start.{}: unknown role (known: \"root\", \"slot\", \"hex\")",
          filename, recipe_id, other
        ));
      }
    }
  }
  Ok(flags)
}

// ---------- Output groups ----------

fn parse_output_groups(
  value: Option<&Value>,
  type_ids: &BTreeMap<String, u8>,
  filename: &str,
  recipe_id: &str,
  field_label: &str,
) -> Result<Vec<ProductGroup>, String> {
  let Some(v) = value else { return Ok(Vec::new()) };
  let obj = v.as_object().ok_or_else(|| {
    format!(
      "{}: recipe {:?}: {} must be an object {{ place: {{ owner: [cards] }} }}",
      filename, recipe_id, field_label
    )
  })?;
  let mut groups: Vec<ProductGroup> = Vec::new();
  for (place_name, place_val) in obj {
    let place = match place_name.as_str() {
      "inventory" => ProductPlace::Inventory,
      "location" => ProductPlace::Location,
      other => {
        return Err(format!(
          "{}: recipe {:?}: {}.{}: unknown place (known: \"inventory\", \"location\")",
          filename, recipe_id, field_label, other
        ));
      }
    };
    let place_obj = place_val.as_object().ok_or_else(|| {
      format!(
        "{}: recipe {:?}: {}.{} must be an object {{ owner: [cards] }}",
        filename, recipe_id, field_label, place_name
      )
    })?;
    for (owner_name, entities_val) in place_obj {
      let owner = match owner_name.as_str() {
        "root" => ProductOwner::Root,
        "actor" => ProductOwner::Actor,
        "hex" => ProductOwner::Hex,
        "action" => ProductOwner::Action,
        other => {
          return Err(format!(
            "{}: recipe {:?}: {}.{}.{}: unknown owner (known: \"root\", \"actor\", \"hex\", \"action\")",
            filename, recipe_id, field_label, place_name, other
          ));
        }
      };
      if place == ProductPlace::Location && owner != ProductOwner::Hex {
        return Err(format!(
          "{}: recipe {:?}: {}.location.{}: only `hex` owner is supported for location outputs",
          filename, recipe_id, field_label, owner_name
        ));
      }
      let arr = entities_val.as_array().ok_or_else(|| {
        format!(
          "{}: recipe {:?}: {}.{}.{} must be an array of card keys",
          filename, recipe_id, field_label, place_name, owner_name
        )
      })?;
      let entities = arr
        .iter()
        .enumerate()
        .map(|(i, ent)| {
          parse_entity(
            ent,
            type_ids,
            filename,
            recipe_id,
            &format!("{}.{}.{}[{}]", field_label, place_name, owner_name, i),
          )
        })
        .collect::<Result<Vec<_>, _>>()?;
      groups.push(ProductGroup {
        target: ProductTarget { place, owner },
        entities,
      });
    }
  }
  Ok(groups)
}

// ---------- Reagents ----------

fn parse_reagents(
  value: Option<&Value>,
  type_ids: &BTreeMap<String, u8>,
  filename: &str,
  recipe_id: &str,
) -> Result<Reagents, String> {
  let Some(v) = value else { return Ok(Reagents::default()) };
  let obj = v.as_object().ok_or_else(|| {
    format!(
      "{}: recipe {:?}: 'reagents' must be an object with optional 'slots' / 'roles' / 'has' / 'has_below'",
      filename, recipe_id
    )
  })?;
  for k in obj.keys() {
    if !matches!(k.as_str(), "slots" | "roles" | "has" | "has_below") {
      return Err(format!(
        "{}: recipe {:?}: reagents.{}: unknown key (known: \"slots\", \"roles\", \"has\", \"has_below\")",
        filename, recipe_id, k
      ));
    }
  }

  let mut slots: Vec<Reagent> = Vec::new();
  if let Some(slots_val) = obj.get("slots") {
    let arr = slots_val.as_array().ok_or_else(|| {
      format!(
        "{}: recipe {:?}: reagents.slots must be an array of 0-indexed slot positions",
        filename, recipe_id
      )
    })?;
    for v in arr {
      let n = v.as_u64().ok_or_else(|| {
        format!(
          "{}: recipe {:?}: reagents.slots[{}]: not a non-negative integer",
          filename, recipe_id, v
        )
      })?;
      if n > u8::MAX as u64 {
        return Err(format!(
          "{}: recipe {:?}: reagents.slots {} exceeds u8 max",
          filename, recipe_id, n
        ));
      }
      slots.push(Reagent::Slot(n as u8));
    }
  }
  if let Some(roles_val) = obj.get("roles") {
    let arr = roles_val.as_array().ok_or_else(|| {
      format!(
        "{}: recipe {:?}: reagents.roles must be an array of role names",
        filename, recipe_id
      )
    })?;
    for v in arr {
      let s = v.as_str().ok_or_else(|| {
        format!(
          "{}: recipe {:?}: reagents.roles[{:?}]: not a string",
          filename, recipe_id, v
        )
      })?;
      let r = match s {
        "root" => Reagent::Root,
        "hex" => Reagent::Hex,
        other => {
          return Err(format!(
            "{}: recipe {:?}: reagents.roles[{:?}]: unknown role (known: \"root\", \"hex\")",
            filename, recipe_id, other
          ));
        }
      };
      slots.push(r);
    }
  }

  let has = parse_has_ops(obj.get("has"), type_ids, filename, recipe_id, "reagents.has")?;
  let has_below = parse_has_ops(
    obj.get("has_below"),
    type_ids,
    filename,
    recipe_id,
    "reagents.has_below",
  )?;
  Ok(Reagents { slots, has, has_below })
}

// ---------- Has ops ----------

fn parse_has_ops(
  value: Option<&Value>,
  type_ids: &BTreeMap<String, u8>,
  filename: &str,
  recipe_id: &str,
  path_label: &str,
) -> Result<HasOps, String> {
  let Some(v) = value else { return Ok(HasOps::default()) };
  let obj = v.as_object().ok_or_else(|| {
    format!(
      "{}: recipe {:?}: {} must be an object with \"root\" / \"actor\" lists",
      filename, recipe_id, path_label
    )
  })?;
  let mut ops = HasOps::default();
  for (k, v) in obj {
    match k.as_str() {
      "root" | "actor" => {
        let arr = v.as_array().ok_or_else(|| {
          format!(
            "{}: recipe {:?}: {}.{} must be an array of entities",
            filename, recipe_id, path_label, k
          )
        })?;
        let entries = arr
          .iter()
          .enumerate()
          .map(|(i, ent)| {
            parse_entity(
              ent,
              type_ids,
              filename,
              recipe_id,
              &format!("{}.{}[{}]", path_label, k, i),
            )
          })
          .collect::<Result<Vec<_>, _>>()?;
        if k == "root" {
          ops.root = entries;
        } else {
          ops.actor = entries;
        }
      }
      other => {
        return Err(format!(
          "{}: recipe {:?}: {}.{}: unknown role (known: \"root\", \"actor\")",
          filename, recipe_id, path_label, other
        ));
      }
    }
  }
  Ok(ops)
}

// ---------- Entity parsing ----------

/// Reserved bare-string sentinel — `"any"` parses as `Entity::Any`.
const ENTITY_ANY_LITERAL: &str = "any";
/// Bare strings starting with `'@'` are card-type sentinels:
/// `"@discipline"` → `Entity::Type(type_id)`.
const ENTITY_TYPE_PREFIX: char = '@';

fn parse_entity(
  value: &Value,
  type_ids: &BTreeMap<String, u8>,
  filename: &str,
  recipe_id: &str,
  path: &str,
) -> Result<Entity, String> {
  // String sugar: card-key, `"any"`, or `"@<type>"`.
  if let Some(s) = value.as_str() {
    if s == ENTITY_ANY_LITERAL {
      return Ok(Entity::Any);
    }
    if let Some(type_name) = s.strip_prefix(ENTITY_TYPE_PREFIX) {
      let &type_id = type_ids.get(type_name).ok_or_else(|| {
        format!(
          "{}: recipe {:?} {}: unknown card type {:?} (not in cards/types.json)",
          filename, recipe_id, path, type_name
        )
      })?;
      return Ok(Entity::Type(type_id));
    }
    return Ok(Entity::Card(s.to_string()));
  }

  // Array sugar: OR-list of entities.
  if let Some(arr) = value.as_array() {
    let children: Vec<Entity> = arr
      .iter()
      .enumerate()
      .map(|(i, v)| parse_entity(v, type_ids, filename, recipe_id, &format!("{}[{}]", path, i)))
      .collect::<Result<_, _>>()?;
    return Ok(match children.len() {
      0 => {
        return Err(format!(
          "{}: recipe {:?} {}: empty OR-list (use at least one entity)",
          filename, recipe_id, path
        ));
      }
      1 => children.into_iter().next().unwrap(),
      _ => Entity::Or(children),
    });
  }

  // Tagged object: dispatch on first recognized key.
  let obj = value.as_object().ok_or_else(|| {
    format!(
      "{}: recipe {:?} {}: entity not a string, array, or object: {:?}",
      filename, recipe_id, path, value
    )
  })?;

  if let Some(card_val) = obj.get("card") {
    let s = card_val.as_str().ok_or_else(|| {
      format!("{}: recipe {:?} {}: 'card' must be a string", filename, recipe_id, path)
    })?;
    return Ok(Entity::Card(s.to_string()));
  }
  if let Some(aspect_val) = obj.get("aspect") {
    let name = aspect_val.as_str().ok_or_else(|| {
      format!("{}: recipe {:?} {}: 'aspect' must be a string", filename, recipe_id, path)
    })?;
    let id = aspect_id(name)?.ok_or_else(|| {
      format!(
        "{}: recipe {:?} {}: unknown aspect {:?} (not in aspects.json)",
        filename, recipe_id, path, name
      )
    })?;
    let min = obj
      .get("min")
      .map(|v| {
        v.as_i64().ok_or_else(|| {
          format!(
            "{}: recipe {:?} {}: aspect.min not an integer: {:?}",
            filename, recipe_id, path, v
          )
        })
      })
      .transpose()?
      .unwrap_or(1) as i32;
    return Ok(Entity::Aspect(id, min));
  }
  if let Some(cat_val) = obj.get("category") {
    let name = cat_val.as_str().ok_or_else(|| {
      format!("{}: recipe {:?} {}: 'category' must be a string", filename, recipe_id, path)
    })?;
    let &id = crate::definition_core::card_category_ids()?.get(name).ok_or_else(|| {
      format!(
        "{}: recipe {:?} {}: unknown category {:?} (not in cards/types.json)",
        filename, recipe_id, path, name
      )
    })?;
    return Ok(Entity::Category(id));
  }
  if let Some(flag_val) = obj.get("flag") {
    let name = flag_val.as_str().ok_or_else(|| {
      format!("{}: recipe {:?} {}: 'flag' must be a string", filename, recipe_id, path)
    })?;
    let bit = card_flag_bit(name)?.ok_or_else(|| {
      format!(
        "{}: recipe {:?} {}: unknown flag {:?} (not in cards/flags.json)",
        filename, recipe_id, path, name
      )
    })?;
    return Ok(Entity::Flag(bit));
  }
  if let Some(any_val) = obj.get("any") {
    let b = any_val.as_bool().ok_or_else(|| {
      format!("{}: recipe {:?} {}: 'any' must be a boolean", filename, recipe_id, path)
    })?;
    if !b {
      return Err(format!(
        "{}: recipe {:?} {}: 'any': false has no meaning — use a real predicate",
        filename, recipe_id, path
      ));
    }
    return Ok(Entity::Any);
  }
  if let Some(and_val) = obj.get("and") {
    let arr = and_val.as_array().ok_or_else(|| {
      format!("{}: recipe {:?} {}: 'and' must be an array of entities", filename, recipe_id, path)
    })?;
    let children: Vec<Entity> = arr
      .iter()
      .enumerate()
      .map(|(i, v)| parse_entity(v, type_ids, filename, recipe_id, &format!("{}.and[{}]", path, i)))
      .collect::<Result<_, _>>()?;
    if children.is_empty() {
      return Err(format!("{}: recipe {:?} {}: empty 'and' list", filename, recipe_id, path));
    }
    return Ok(Entity::And(children));
  }
  if let Some(or_val) = obj.get("or") {
    let arr = or_val.as_array().ok_or_else(|| {
      format!("{}: recipe {:?} {}: 'or' must be an array of entities", filename, recipe_id, path)
    })?;
    let children: Vec<Entity> = arr
      .iter()
      .enumerate()
      .map(|(i, v)| parse_entity(v, type_ids, filename, recipe_id, &format!("{}.or[{}]", path, i)))
      .collect::<Result<_, _>>()?;
    if children.is_empty() {
      return Err(format!("{}: recipe {:?} {}: empty 'or' list", filename, recipe_id, path));
    }
    return Ok(Entity::Or(children));
  }
  if let Some(not_val) = obj.get("not") {
    let child = parse_entity(not_val, type_ids, filename, recipe_id, &format!("{}.not", path))?;
    return Ok(Entity::Not(Box::new(child)));
  }

  Err(format!(
    "{}: recipe {:?} {}: object has no recognized entity key (expected one of: card, aspect, category, flag, any, and, or, not); got keys {:?}",
    filename, recipe_id, path, obj.keys().collect::<Vec<_>>()
  ))
}

// ---------- Duration parsing ----------

fn parse_duration(
  value: &Value,
  type_ids: &BTreeMap<String, u8>,
  filename: &str,
  recipe_id: &str,
) -> Result<Duration, String> {
  if let Some(n) = value.as_u64() {
    return Ok(Duration::Fixed(n as u32));
  }
  let arr = value.as_array().ok_or_else(|| {
    format!(
      "{}: recipe {:?}: 'duration' must be a non-negative integer or an array of tier objects",
      filename, recipe_id
    )
  })?;
  if arr.is_empty() {
    return Err(format!("{}: recipe {:?}: duration is an empty array", filename, recipe_id));
  }
  let mut cases: Vec<(u32, Entity)> = Vec::new();
  let mut fallback: Option<u32> = None;
  for (i, tier) in arr.iter().enumerate() {
    let tier_obj = tier.as_object().ok_or_else(|| {
      format!(
        "{}: recipe {:?}: duration[{}]: not an object {{ when?, seconds }}",
        filename, recipe_id, i
      )
    })?;
    let secs = tier_obj
      .get("seconds")
      .and_then(Value::as_u64)
      .ok_or_else(|| {
        format!(
          "{}: recipe {:?}: duration[{}]: 'seconds' missing or not a non-negative integer",
          filename, recipe_id, i
        )
      })? as u32;
    match tier_obj.get("when") {
      None => {
        if i != arr.len() - 1 {
          return Err(format!(
            "{}: recipe {:?}: duration[{}]: tier without 'when' must be the trailing fallback",
            filename, recipe_id, i
          ));
        }
        fallback = Some(secs);
      }
      Some(when_val) => {
        let cond = parse_entity(
          when_val,
          type_ids,
          filename,
          recipe_id,
          &format!("duration[{}].when", i),
        )?;
        cases.push((secs, cond));
      }
    }
  }
  let fallback = fallback.ok_or_else(|| {
    format!(
      "{}: recipe {:?}: duration: last tier must omit 'when' to act as the fallback",
      filename, recipe_id
    )
  })?;
  Ok(Duration::Conditional { cases, fallback })
}
