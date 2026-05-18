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
//!     "self":     { ... }
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
//! `self` for on_create). Third-level keys ARE the recipe
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
//!   (`{"category": ...}` retired with the card-category dimension.)
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
//! - **`output`**: `{ "<place>": { "<owner>":
//!   [<card_key>, ...] } }`. Place is `inventory` or `location`. Owner
//!   is `root`, `actor`, `hex`, or `action`. Spawn lists are flat
//!   card-key arrays (not predicates — they're constructors).
//! - **`hex`** / **`root`**: single entity (or OR-list-sugar array).
//! - **`slots`**: array of slots, each slot is an entity.
//! - **`has`** / **`has_below`**: `{ "root"?: [entity, ...], "actor"?:
//!   [entity, ...] }` — each entry is a slot on the named soul-stack.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::definition_core::{aspect_id, card_type_ids, decode_definition, AspectId, CardDefinition};
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
  /// window against. Failure outcomes for magnetic anchors are
  /// authored as regular stack recipes targeting the magnetic card
  /// as root.
  Stack(StackDirection),
  /// `on_create.self` — fired when a card is inserted; the new card
  /// is both root and actor of the resulting action.
  OnCreate,
  /// `magnetic.up` / `magnetic.down` — success path for a
  /// lifecycle-pending card. Looked up by stable id (stored on the
  /// card's def as `lifecycle_recipe_key`) and matched by
  /// [`match_magnetic_recipe`]. Never appears via the regular
  /// stack-recipe candidate walk. Phase 6 of the lifecycle rewrite
  /// folds this into `Stack(_)`.
  Magnetic(StackDirection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
  Up,
  Down,
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

/// Role of a [`ConsumeStock`] target. v1 only supports `Hex` —
/// mutating a stock slot on the recipe's hex tile. `Root` /
/// `Slots[i]` are reserved for future use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeRole {
  Hex,
}

/// Op applied by a [`ConsumeStock`] entry. `Sub` saturates at 0,
/// `Add` saturates at the slot's declared `max`, `Set` writes the
/// literal value clamped to `[0, max]`. Parsed from
/// `output.modify.<ref>.aspect.<name>.<op>: N`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockOp {
  Add,
  Sub,
  Set,
}

/// One stock-mutation clause on a [`RecipeDef`]. Parsed from
/// `output.modify.<ref>.aspect.<name>.<op>: N` — at completion time
/// `action_completion::apply` walks each entry, looks up the
/// target's stock slot for `aspect_id`, and applies the op against
/// the row value with saturating semantics.
///
/// See [docs/TILE_ASPECTS.md] §"Recipe stock consumption" and
/// `output.modify` in [content/recipes/AGENTS.md].
#[derive(Debug, Clone)]
pub struct ConsumeStock {
  pub role: ConsumeRole,
  pub aspect_id: AspectId,
  pub op: StockOp,
  pub delta: u8,
}

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
  pub duration: Option<Duration>,
  pub set_start: SetStartFlags,
  /// `progress_style` field value (0..=7); see `cards/flags.json`.
  pub style: u8,
  /// Stock-decrement clauses applied at completion. Empty for recipes
  /// that don't touch row-mutable aspect values. See [`ConsumeStock`].
  pub consume: Vec<ConsumeStock>,
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

/// Try to match a regular `Stack(direction)` recipe against a stack.
/// Returns the packed recipe index of the best match, or `0` if no
/// recipe matches.
///
/// **Caller responsibility — `FLAG_MAGNETIC_HOLD` exclusion.** This
/// matcher operates on `packed_definition` values; it has no knowledge
/// of runtime card flags. Callers must **not** include cards carrying
/// `FLAG_MAGNETIC_HOLD` in `slot_defs` or pass one as `root_def` —
/// magnetic-flagged cards belong to magnetic recipes (see
/// [`match_magnetic_recipe`]) and would otherwise be erroneously
/// considered for regular stack matching. The exclusion lives in
/// callers because they're the ones with access to the actual `Card`
/// row's `flags`. See `actions::propose_action` (server) and the
/// client-side recipe scan path for the canonical filters.
pub fn match_stack_recipe(
  hex_def: u16,
  hex_stocks: Option<(u8, u8)>,
  root_def: u16,
  slot_defs: &[u16],
  direction: StackDirection,
) -> Result<u16, String> {
  Ok(
    match_stack_recipe_detail(hex_def, hex_stocks, root_def, slot_defs, direction, None)?
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
  hex_stocks: Option<(u8, u8)>,
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
        // Hex-tier eval is stock-aware: forest tiles, mountain tiles,
        // etc. carry row-mutable aspect values in `def.stock` rather
        // than in static `def.aspects`. Slot / root predicates stay
        // stocks-less (inventory cards don't have row stocks today).
        let s = entity_specificity_with_stocks(e, def, hex_stocks);
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

/// Try to match a magnetic recipe against a (root, slots) stack. The
/// caller is responsible for having determined that the root card
/// carries `FLAG_MAGNETIC_HOLD` and that the recipe direction matches
/// the player's intent (up vs down).
///
/// Unlike [`match_stack_recipe_detail`], this is a **direct lookup**
/// — the matcher doesn't search all magnetic recipes for the best
/// fit. Instead it reads the root's `lifecycle_recipe_key`, resolves
/// to the specific recipe, validates predicates against the supplied
/// stack, and either returns a [`StackMatch`] or `Ok(None)` if the
/// recipe's predicates aren't satisfied.
///
/// Returns:
/// - `Ok(Some(StackMatch))` when the magnetic recipe matches.
/// - `Ok(None)` when the root has no `lifecycle_recipe_key` (not a
///   lifecycle-pending card), when the slot predicates don't match,
///   or when the recipe's declared direction doesn't match the
///   caller's `direction`.
/// - `Err` when the def or recipe registry fails to build, or when
///   the recipe key on the def points at a non-magnetic or
///   nonexistent recipe (an authoring error caught by
///   [`crate::definition_core::validate_lifecycle_recipes`] at test
///   time, but defended here too).
pub fn match_magnetic_recipe(
  root_def: u16,
  slot_defs: &[u16],
  direction: StackDirection,
  has_candidates: Option<&HasCandidates>,
) -> Result<Option<StackMatch>, String> {
  let Some(root_card) = decode_definition(root_def)? else {
    return Ok(None);
  };
  let Some(packed_recipe_id) =
    crate::definition_core::lifecycle_recipe_for_def(root_card)?
  else {
    return Ok(None);
  };
  let Some(recipe) = recipe(packed_recipe_id)? else {
    return Err(format!(
      "card {:?}: magnetic_recipe resolved to packed id {} which isn't registered",
      root_card.key, packed_recipe_id
    ));
  };

  // Confirm direction matches. A magnetic_up card paired with a
  // downward stack-build attempt is a no-match (returns `Ok(None)`),
  // not an error — the client could be trying both directions.
  match recipe.recipe_type {
    RecipeType::Magnetic(d) if d == direction => {}
    RecipeType::Magnetic(_) => return Ok(None),
    other => {
      return Err(format!(
        "card {:?}: magnetic_recipe resolves to non-magnetic type {:?}",
        root_card.key, other
      ));
    }
  }

  // Validate predicates exactly like match_stack_recipe_detail does,
  // but against this one recipe rather than searching. Magnetic
  // recipes don't declare a `hex` entity today — they pull from
  // inventory, not from a hex tile. If a future content extension
  // adds hex predicates to magnetic recipes, this branch needs to
  // grow accordingly; for now we treat any declared hex as a
  // no-match here (recipes/data validation should reject hex on
  // magnetic recipes upstream).
  if recipe.hex.is_some() {
    return Ok(None);
  }

  if let Some(e) = &recipe.root {
    if entity_specificity(e, root_card) == 0 {
      return Ok(None);
    }
  }

  if let Some(hc) = has_candidates {
    if !has_predicates_feasible(recipe, hc) {
      return Ok(None);
    }
  }

  // Slot predicates: magnetic recipes pull cards in declared order
  // (slot[0], slot[1], ...). The caller is expected to pass the same
  // ordering in `slot_defs`. Specificity ranking isn't useful here
  // (no search across alternatives) but we still compute it so the
  // returned `StackMatch` carries comparable shape to the regular
  // matcher's output.
  if slot_defs.len() != recipe.slots.len() {
    return Ok(None);
  }
  // No specificity accumulation needed — there's no ranking across
  // alternatives, just a single recipe to validate against.
  for (slot_entity, &slot_def) in recipe.slots.iter().zip(slot_defs.iter()) {
    let Some(def) = decode_definition(slot_def)? else {
      return Ok(None);
    };
    if entity_specificity(slot_entity, def) == 0 {
      return Ok(None);
    }
  }

  Ok(Some(StackMatch {
    recipe_index: recipe.index,
    slot_start: 0,
    slot_count: recipe.slots.len() as u32,
    has_root: recipe.root.is_some(),
    has_hex: false,
  }))
}

/// Score how well a single entity matches a card definition; 0 means
/// no match. See [`Entity`] for weight conventions.
pub fn entity_specificity(entity: &Entity, def: &CardDefinition) -> u32 {
  entity_specificity_with_stocks(entity, def, None)
}

/// Stock-aware variant of [`entity_specificity`]. When `stocks` is
/// `Some((stock0, stock1))` and `def` declares a `stock` slot whose
/// aspect descends from an `Entity::Aspect` target, the row stock
/// value takes priority over the def's static `aspects` map (mirrors
/// `docs/TILE_ASPECTS.md` § "Recipe matching"). Sub-aspect widening
/// applies symmetrically: a stock slot declared `pine` satisfies a
/// `wood.min: N` predicate because pine descends from wood. When no
/// stock slot covers the predicate's aspect tree, the evaluator
/// falls back to the static-aspect sum.
///
/// Stocks are passed only to the hex predicate eval in
/// [`match_stack_recipe_detail`] — slot and root predicates ignore
/// `stocks` since inventory cards don't carry row-mutable aspects in
/// v1. The combinators (`And`/`Or`/`Not`/`WeightedOr`) thread
/// `stocks` through unchanged.
pub fn entity_specificity_with_stocks(
  entity: &Entity,
  def: &CardDefinition,
  stocks: Option<(u8, u8)>,
) -> u32 {
  match entity {
    Entity::Card(key) => {
      if &def.key == key { 4 } else { 0 }
    }
    Entity::Aspect(aspect, min) => {
      use crate::definition_core::is_aspect_descendant;
      // Sub-aspect widening: a def declaring `berries: 2` or carrying
      // `berries` in stock satisfies `Entity::Aspect(food, 1)` because
      // berries → ... → food. `is_aspect_descendant` returns true for
      // the trivial self-case too.
      //
      // Stock takes priority when present: if ANY stock slot's aspect
      // descends from the predicate target, the row stock total is
      // authoritative — even when stock=0 with a non-zero static
      // aspect entry on the same descendant. Depletion semantics: a
      // chopped-down forest tile (pine stock = 0) should fail a
      // `wood.min: 1` predicate even if its def still lists a
      // structural wood aspect.
      if let Some((s0, s1)) = stocks {
        let mut had_match = false;
        let mut stock_total: i32 = 0;
        for (idx, slot) in def.stock.iter().enumerate() {
          if is_aspect_descendant(slot.aspect_id, *aspect).unwrap_or(false) {
            had_match = true;
            let row_val = if idx == 0 { s0 } else { s1 } as i32;
            stock_total += row_val;
          }
        }
        if had_match {
          return if stock_total >= *min { 3 } else { 0 };
        }
      }
      // No stock slot covers the predicate's aspect tree (or no row
      // stocks supplied at all) — fall back to summing static aspect
      // entries the same way.
      let val: i32 = def
        .aspects
        .iter()
        .filter(|(a, _)| is_aspect_descendant(*a, *aspect).unwrap_or(false))
        .map(|(_, v)| *v)
        .sum();
      if val >= *min { 3 } else { 0 }
    }
    Entity::Type(type_id) => {
      if def.card_type == *type_id { 2 } else { 0 }
    }
    Entity::Flag(bit) => {
      if def.flags & (1u32 << bit) != 0 { 2 } else { 0 }
    }
    Entity::Any => 1,
    Entity::And(children) => {
      let mut sum: u32 = 0;
      for c in children {
        let s = entity_specificity_with_stocks(c, def, stocks);
        if s == 0 {
          return 0;
        }
        sum = sum.saturating_add(s);
      }
      sum
    }
    Entity::Or(children) => children
      .iter()
      .map(|c| entity_specificity_with_stocks(c, def, stocks))
      .max()
      .unwrap_or(0),
    Entity::Not(child) => {
      if entity_specificity_with_stocks(child, def, stocks) == 0 { 1 } else { 0 }
    }
    Entity::WeightedOr { a, b, .. } => {
      entity_specificity_with_stocks(a, def, stocks)
        .max(entity_specificity_with_stocks(b, def, stocks))
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
    RecipeType::Magnetic(StackDirection::Up) => ("magnetic", "up"),
    RecipeType::Magnetic(StackDirection::Down) => ("magnetic", "down"),
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

  for (filename, content) in RECIPES_FILES {
    let parsed: Value = serde_json::from_str(content)
      .map_err(|e| format!("{}: parse failed: {}", filename, e))?;
    let root = parsed.as_object().ok_or_else(|| {
      format!(
        "{}: top-level must be an object keyed by `<type>` or `<type>.<category>`",
        filename
      )
    })?;

    for (type_key, by_key_val) in root {
      // Outer file keys are dotted (`stack.up`, `magnetic.up`, bare
      // `on_create`) — see [`crate::recipe_statement::parse_recipe_type_key`].
      // Unknown keys (including the `_comment` JSON-doc convention)
      // are silently skipped so content files can outpace the Rust
      // enum, matching the cards loader's discipline.
      let Some(recipe_type) = crate::recipe_statement::parse_recipe_type_key(type_key) else {
        continue;
      };
      let pair = recipe_type_pair(recipe_type)?;
      let recipes_obj = by_key_val.as_object().ok_or_else(|| {
        format!(
          "{}: {}: value not an object of recipe keys",
          filename, type_key
        )
      })?;

      for (key, recipe_value) in recipes_obj {
        let stable_id = *packed_ids.get(key).ok_or_else(|| {
          format!(
            "{}: recipe {:?} ({}) not in recipes/id.json — run gen-ids.py",
            filename, key, type_key
          )
        })?;
        if id_by_name.contains_key(key) {
          return Err(format!(
            "{}: recipe key {:?} declared more than once",
            filename, key
          ));
        }
        let def =
          parse_recipe_flat(key, recipe_value, recipe_type, stable_id, filename, &type_ids)?;
        by_type.entry(pair).or_default().push(stable_id);
        id_by_name.insert(key.clone(), stable_id);
        by_id.insert(stable_id, def);
      }
    }
  }

  Ok(RecipeRegistry { by_id, id_by_name, by_type })
}

// ---------- Per-recipe parsing ----------

/// Parse a recipe whose body is a JSON array of statement strings
/// (the flat shape). See [content/recipes/AGENTS.md](../recipes/AGENTS.md)
/// for the full grammar.
///
/// Each statement is `<path>` (verb-only) or `<path>: <value>` —
/// see [`crate::recipe_statement::parse_statement`] for the
/// lexical layer. This function walks the parsed statements,
/// dispatches on the first segment (`input` / `output` / `duration`
/// / `style`), and builds the same `RecipeDef` shape the legacy
/// nested-object parser produced, so downstream consumers
/// (matchers, action_completion, magnetic) are untouched.
fn parse_recipe_flat(
  key: &str,
  recipe_value: &Value,
  recipe_type: RecipeType,
  stable_id: u16,
  filename: &str,
  type_ids: &BTreeMap<String, u8>,
) -> Result<RecipeDef, String> {
  use crate::recipe_statement::{parse_statement, StatementValue};

  let arr = recipe_value.as_array().ok_or_else(|| {
    format!(
      "{}: recipe {:?}: value must be a JSON array of statement strings",
      filename, key
    )
  })?;

  // Pre-parse every statement so the per-bucket walkers dispatch on
  // already-tokenised paths instead of re-parsing the strings.
  let mut statements: Vec<crate::recipe_statement::Statement> = Vec::with_capacity(arr.len());
  for (i, raw) in arr.iter().enumerate() {
    let s = raw.as_str().ok_or_else(|| {
      format!(
        "{}: recipe {:?}: statement[{}] must be a string, got {:?}",
        filename, key, i, raw
      )
    })?;
    let stmt = parse_statement(s).map_err(|e| {
      format!("{}: recipe {:?}: statement[{}]: {}", filename, key, i, e)
    })?;
    statements.push(stmt);
  }

  // Field accumulators — mirror the `RecipeDef` shape produced by
  // the legacy parser so downstream consumers see no difference.
  let mut root_entity: Option<Entity> = None;
  let mut hex_entity: Option<Entity> = None;
  let mut slots_entities: Vec<Option<Entity>> = Vec::new();
  let mut has = HasOps::default();
  let has_below = HasOps::default();
  let mut reagent_slots: Vec<Reagent> = Vec::new();
  let mut reagent_has_root = false;
  let mut reagent_has_hex = false;
  let mut output_groups: Vec<ProductGroup> = Vec::new();
  let mut consume_stocks: Vec<ConsumeStock> = Vec::new();
  let mut style: u8 = 0;
  let mut duration_default: Option<u32> = None;
  let mut duration_tiers: Vec<(u32, Entity)> = Vec::new();

  for (idx, stmt) in statements.iter().enumerate() {
    let stmt_label = || format!("{}: recipe {:?}: statement[{}]", filename, key, idx);
    let bucket = stmt
      .bucket()
      .ok_or_else(|| format!("{}: empty path", stmt_label()))?;
    let tail = &stmt.path[1..];
    match bucket {
      "input" => apply_input_statement(
        tail,
        stmt.value.as_ref(),
        type_ids,
        &stmt_label,
        &mut root_entity,
        &mut hex_entity,
        &mut slots_entities,
        &mut has,
      )?,
      "output" => apply_output_statement(
        tail,
        stmt.value.as_ref(),
        type_ids,
        &stmt_label,
        hex_entity.as_ref(),
        &mut reagent_slots,
        &mut reagent_has_root,
        &mut reagent_has_hex,
        &mut output_groups,
        &mut consume_stocks,
      )?,
      "duration" => apply_duration_statement(
        tail,
        stmt.value.as_ref(),
        type_ids,
        &stmt_label,
        &mut duration_default,
        &mut duration_tiers,
      )?,
      "style" => {
        if tail.len() != 1 || tail[0].as_word() != Some("default") {
          return Err(format!(
            "{}: style only accepts `style.default: <name>`",
            stmt_label()
          ));
        }
        let Some(StatementValue::Str(s)) = stmt.value.as_ref() else {
          return Err(format!(
            "{}: style.default needs a string value (none / ltr / rtl)",
            stmt_label()
          ));
        };
        style = parse_style_name(s).ok_or_else(|| {
          format!(
            "{}: style.default {:?} not recognised — use none / ltr / rtl",
            stmt_label(),
            s
          )
        })?;
      }
      other => {
        return Err(format!(
          "{}: unknown top-level bucket {:?} (expected input / output / duration / style)",
          stmt_label(),
          other
        ));
      }
    }
  }

  // Materialise slots, refusing gaps. A recipe that mentions
  // `slot.0` and `slot.2` without `slot.1` is almost certainly an
  // author error.
  let mut slots: Vec<Entity> = Vec::with_capacity(slots_entities.len());
  for (i, slot) in slots_entities.iter().enumerate() {
    let Some(entity) = slot else {
      return Err(format!(
        "{}: recipe {:?}: slot[{}] referenced but never assigned a predicate",
        filename, key, i
      ));
    };
    slots.push(entity.clone());
  }

  let mut reagents = Reagents::default();
  reagents.slots = reagent_slots;
  if reagent_has_root {
    // Reagent role consumption preserved by replaying the role into
    // the legacy `Reagents.slots` channel — `Reagent::Root` is the
    // existing carrier for "the chain root dies." Same for hex.
    reagents.slots.push(Reagent::Root);
  }
  if reagent_has_hex {
    reagents.slots.push(Reagent::Hex);
  }

  let duration_value = if !duration_tiers.is_empty() {
    let fallback = duration_default.ok_or_else(|| {
      format!(
        "{}: recipe {:?}: duration has when-tiers but no `duration.default` fallback",
        filename, key
      )
    })?;
    Some(Duration::Conditional {
      cases: duration_tiers,
      fallback,
    })
  } else {
    duration_default.map(Duration::Fixed)
  };
  if duration_value.is_none() {
    return Err(format!(
      "{}: recipe {:?}: missing required `duration.default: <seconds>`",
      filename, key
    ));
  }

  if matches!(recipe_type, RecipeType::OnCreate)
    && root_entity.is_none()
    && hex_entity.is_none()
  {
    return Err(format!(
      "{}: recipe {:?}: on_create recipes must declare `input.root.*` or `input.hex.*`",
      filename, key
    ));
  }

  Ok(RecipeDef {
    index: stable_id,
    id: key.to_string(),
    recipe_type,
    root: root_entity,
    hex: hex_entity,
    slots,
    reagents,
    has,
    has_below,
    output: output_groups,
    duration: duration_value,
    set_start: SetStartFlags::default(),
    style,
    consume: consume_stocks,
  })
}

// ---------- Per-bucket statement appliers --------------------------------

fn parse_style_name(s: &str) -> Option<u8> {
  match s {
    "none" => Some(0),
    "ltr" => Some(1),
    "rtl" => Some(2),
    _ => None,
  }
}

/// Resolve an aspect-name segment against the registry. Surfaces a
/// good error message naming the offending recipe statement.
fn resolve_aspect_segment(
  name: &str,
  stmt_label: &dyn Fn() -> String,
) -> Result<AspectId, String> {
  if crate::recipe_statement::is_reserved_aspect_name(name) {
    return Err(format!(
      "{}: aspect name {:?} collides with a reserved path token — \
       rename the aspect in aspects.json",
      stmt_label(),
      name
    ));
  }
  match aspect_id(name).map_err(|e| format!("{}: {}", stmt_label(), e))? {
    Some(id) => Ok(id),
    None => Err(format!(
      "{}: aspect {:?} not registered in aspects.json",
      stmt_label(),
      name
    )),
  }
}

/// Parse an `aspect.<name>.min` suffix on a path tail. Returns the
/// resolved `Entity::Aspect`. The statement's integer value is the
/// `min` threshold. Errors on malformed suffixes.
fn entity_from_aspect_min_tail(
  tail: &[crate::recipe_statement::Segment],
  value: Option<&crate::recipe_statement::StatementValue>,
  stmt_label: &dyn Fn() -> String,
) -> Result<Entity, String> {
  // Expected suffix: `aspect.<name>.min`
  if tail.len() != 3 {
    return Err(format!(
      "{}: aspect predicate must be `aspect.<name>.min: <int>`",
      stmt_label()
    ));
  }
  if tail[0].as_word() != Some("aspect") {
    return Err(format!(
      "{}: expected `aspect.<name>.min`, got {:?}",
      stmt_label(),
      tail[0]
    ));
  }
  let name = tail[1].as_word().ok_or_else(|| {
    format!("{}: aspect name must be a word, got {:?}", stmt_label(), tail[1])
  })?;
  if tail[2].as_word() != Some("min") {
    return Err(format!(
      "{}: aspect predicate suffix must be `min`, got {:?} (v1 supports min only)",
      stmt_label(),
      tail[2]
    ));
  }
  let aspect = resolve_aspect_segment(name, stmt_label)?;
  let min = value
    .and_then(crate::recipe_statement::StatementValue::as_int)
    .ok_or_else(|| {
      format!("{}: aspect.{}.min needs an integer value", stmt_label(), name)
    })?;
  Ok(Entity::Aspect(aspect, min as i32))
}

/// Parse a `def_id` suffix on a path tail. Returns the resolved
/// `Entity::Card`. The statement's string value is the card key.
fn entity_from_def_id_tail(
  tail: &[crate::recipe_statement::Segment],
  value: Option<&crate::recipe_statement::StatementValue>,
  stmt_label: &dyn Fn() -> String,
) -> Result<Entity, String> {
  if tail.len() != 1 || tail[0].as_word() != Some("def_id") {
    return Err(format!(
      "{}: expected `def_id: <key>`, got tail {:?}",
      stmt_label(),
      tail
    ));
  }
  let s = value
    .and_then(crate::recipe_statement::StatementValue::as_str)
    .ok_or_else(|| format!("{}: def_id needs a string value", stmt_label()))?;
  Ok(Entity::Card(s.to_string()))
}

/// Target an input statement points at, after the anchor + optional
/// slot index are consumed.
enum InputTarget {
  Root,
  Hex,
  Slot(usize),
  HasActor,
}

/// Consume the anchor (and slot index, if present) off the head of
/// `tail` and return `(target, entity-suffix tail)`. `slot.<i>` and
/// `actor` (sugar for `slot.0`) both produce `InputTarget::Slot`.
fn split_input_anchor<'a>(
  tail: &'a [crate::recipe_statement::Segment],
  stmt_label: &dyn Fn() -> String,
) -> Result<(InputTarget, &'a [crate::recipe_statement::Segment]), String> {
  let anchor = tail
    .first()
    .and_then(crate::recipe_statement::Segment::as_word)
    .ok_or_else(|| format!("{}: input.* needs an anchor word", stmt_label()))?;
  match anchor {
    "root" => Ok((InputTarget::Root, &tail[1..])),
    "hex" => Ok((InputTarget::Hex, &tail[1..])),
    "actor" => Ok((InputTarget::Slot(0), &tail[1..])),
    "has" => Ok((InputTarget::HasActor, &tail[1..])),
    "slot" => {
      let i = tail
        .get(1)
        .and_then(crate::recipe_statement::Segment::as_index)
        .ok_or_else(|| format!("{}: slot anchor needs `slot.<i>` index", stmt_label()))?;
      Ok((InputTarget::Slot(i as usize), &tail[2..]))
    }
    other => Err(format!(
      "{}: unknown input anchor {:?} (expected root / hex / slot / actor / has)",
      stmt_label(),
      other
    )),
  }
}

/// Parse one `input.<...>` statement, mutating the recipe's match-
/// side accumulators.
#[allow(clippy::too_many_arguments)]
fn apply_input_statement(
  tail: &[crate::recipe_statement::Segment],
  value: Option<&crate::recipe_statement::StatementValue>,
  _type_ids: &BTreeMap<String, u8>,
  stmt_label: &dyn Fn() -> String,
  root: &mut Option<Entity>,
  hex: &mut Option<Entity>,
  slots: &mut Vec<Option<Entity>>,
  has: &mut HasOps,
) -> Result<(), String> {
  let (target, rest) = split_input_anchor(tail, stmt_label)?;
  let head = rest.first().and_then(crate::recipe_statement::Segment::as_word);
  let entity = match head {
    Some("def_id") => entity_from_def_id_tail(rest, value, stmt_label)?,
    Some("aspect") => entity_from_aspect_min_tail(rest, value, stmt_label)?,
    _ => {
      return Err(format!(
        "{}: input target expects `.def_id: <key>` or `.aspect.<name>.min: <int>`, got tail {:?}",
        stmt_label(),
        rest
      ));
    }
  };

  match target {
    InputTarget::Root => {
      if root.is_some() {
        return Err(format!("{}: input.root.* declared more than once", stmt_label()));
      }
      *root = Some(entity);
    }
    InputTarget::Hex => {
      if hex.is_some() {
        return Err(format!("{}: input.hex.* declared more than once", stmt_label()));
      }
      *hex = Some(entity);
    }
    InputTarget::Slot(i) => {
      if slots.len() <= i {
        slots.resize(i + 1, None);
      }
      if slots[i].is_some() {
        return Err(format!(
          "{}: input.slot.{}.* declared more than once",
          stmt_label(),
          i
        ));
      }
      slots[i] = Some(entity);
    }
    InputTarget::HasActor => {
      // v1: implicit role = actor, direction = above. The entity
      // joins `has.actor`. `input.has.root.*` and `has_below.*`
      // are deferred until a recipe needs them.
      has.actor.push(entity);
    }
  }
  Ok(())
}

// --- Output appliers ---

/// `output.destroy.<card_ref>` resolves to one of the legacy
/// [`Reagent`] variants. `slot.<i>` → `Slot(i)`, `actor` → `Slot(0)`,
/// `root` → `Root`, `hex` → `Hex`. Resolver chains (`.owner`, etc.)
/// are reserved for future use; v1 rejects them with a clear error.
fn parse_destroy_card_ref(
  segs: &[crate::recipe_statement::Segment],
  stmt_label: &dyn Fn() -> String,
) -> Result<Reagent, String> {
  let head = segs
    .first()
    .and_then(crate::recipe_statement::Segment::as_word)
    .ok_or_else(|| format!("{}: destroy needs a card ref", stmt_label()))?;
  match head {
    "root" if segs.len() == 1 => Ok(Reagent::Root),
    "hex" if segs.len() == 1 => Ok(Reagent::Hex),
    "actor" if segs.len() == 1 => Ok(Reagent::Slot(0)),
    "slot" => {
      if segs.len() != 2 {
        return Err(format!(
          "{}: destroy.slot needs `slot.<i>` with index only",
          stmt_label()
        ));
      }
      let i = segs[1].as_index().ok_or_else(|| {
        format!("{}: slot index must be an integer", stmt_label())
      })?;
      Ok(Reagent::Slot(i as u8))
    }
    other => Err(format!(
      "{}: destroy ref {:?} not supported in v1 (expected root / hex / actor / slot.<i>)",
      stmt_label(),
      other
    )),
  }
}

/// Resolve a `create` target ref to a `ProductOwner`. Both `actor`
/// and `slot.0` map to `Actor`; `slot.0.owner` also maps to `Actor`
/// (semantically the actor's container). `root` → `Root`, `hex` →
/// `Hex`. Longer resolver chains reject for v1.
fn parse_create_owner(
  segs: &[crate::recipe_statement::Segment],
  stmt_label: &dyn Fn() -> String,
) -> Result<ProductOwner, String> {
  let words: Vec<&str> = segs
    .iter()
    .filter_map(crate::recipe_statement::Segment::as_word)
    .collect();
  let head = segs.first().and_then(crate::recipe_statement::Segment::as_word);
  match (head, segs.len(), words.last().copied()) {
    (Some("root"), 1, _) => Ok(ProductOwner::Root),
    (Some("hex"), 1, _) => Ok(ProductOwner::Hex),
    (Some("actor"), 1, _) => Ok(ProductOwner::Actor),
    // `actor.owner` is the soul that contains the actor — under
    // today's semantics that's where `Actor`-placed inventory items
    // already land (resolve_owner walks UP via
    // inventory_container_for_at), so the two refs collapse.
    (Some("actor"), 2, Some("owner")) => Ok(ProductOwner::Actor),
    (Some("slot"), 2, _) => {
      // `slot.<i>` — only `slot.0` is meaningful as a create target;
      // higher indices have no defined owner today.
      let i = segs[1].as_index().unwrap_or(u32::MAX);
      if i == 0 {
        Ok(ProductOwner::Actor)
      } else {
        Err(format!(
          "{}: create.slot.{} has no defined owner (only slot.0 = actor is supported in v1)",
          stmt_label(),
          i
        ))
      }
    }
    (Some("slot"), 3, Some("owner")) => {
      let i = segs[1].as_index().unwrap_or(u32::MAX);
      if i == 0 {
        Ok(ProductOwner::Actor)
      } else {
        Err(format!(
          "{}: create.slot.{}.owner has no defined owner (only slot.0.owner = actor's soul is supported in v1)",
          stmt_label(),
          i
        ))
      }
    }
    _ => Err(format!(
      "{}: create ref not supported in v1 (expected root / hex / actor / slot.0 / slot.0.owner)",
      stmt_label()
    )),
  }
}

/// Parse `output.modify.hex.aspect.<name>.<op>` → `(aspect_id, op)`.
/// v1 only accepts `hex` as the target ref. Other refs reject at
/// parse time (per the plan's "card-rooted stock writes" out-of-
/// scope note).
fn parse_modify_target(
  segs: &[crate::recipe_statement::Segment],
  stmt_label: &dyn Fn() -> String,
) -> Result<(AspectId, StockOp), String> {
  // Expect: <ref-segments> . aspect . <name> . <op>
  // v1 ref must be `hex` (single segment).
  if segs.len() < 4 {
    return Err(format!(
      "{}: modify expects `<ref>.aspect.<name>.<op>`",
      stmt_label()
    ));
  }
  if segs[0].as_word() != Some("hex") {
    return Err(format!(
      "{}: modify ref {:?} not supported in v1 (only `hex` has row-mutable storage today)",
      stmt_label(),
      segs[0]
    ));
  }
  if segs[1].as_word() != Some("aspect") {
    return Err(format!(
      "{}: modify expects `hex.aspect.<name>.<op>`, got {:?}",
      stmt_label(),
      segs[1]
    ));
  }
  let aspect_name = segs[2].as_word().ok_or_else(|| {
    format!("{}: aspect name must be a word", stmt_label())
  })?;
  let op_word = segs[3].as_word().ok_or_else(|| {
    format!("{}: modify op must be a word", stmt_label())
  })?;
  if segs.len() > 4 {
    return Err(format!(
      "{}: modify path has trailing segments after the op",
      stmt_label()
    ));
  }
  let aspect = resolve_aspect_segment(aspect_name, stmt_label)?;
  let op = match op_word {
    "add" => StockOp::Add,
    "sub" => StockOp::Sub,
    "set" => StockOp::Set,
    other => {
      return Err(format!(
        "{}: modify op {:?} not recognised (use add / sub / set)",
        stmt_label(),
        other
      ));
    }
  };
  Ok((aspect, op))
}

#[allow(clippy::too_many_arguments)]
fn apply_output_statement(
  tail: &[crate::recipe_statement::Segment],
  value: Option<&crate::recipe_statement::StatementValue>,
  _type_ids: &BTreeMap<String, u8>,
  stmt_label: &dyn Fn() -> String,
  hex_entity: Option<&Entity>,
  reagent_slots: &mut Vec<Reagent>,
  reagent_has_root: &mut bool,
  reagent_has_hex: &mut bool,
  output_groups: &mut Vec<ProductGroup>,
  consume_stocks: &mut Vec<ConsumeStock>,
) -> Result<(), String> {
  let head = tail
    .first()
    .and_then(crate::recipe_statement::Segment::as_word)
    .ok_or_else(|| format!("{}: output.* needs a sub-bucket", stmt_label()))?;
  let body = &tail[1..];
  match head {
    "destroy" => {
      let reagent = parse_destroy_card_ref(body, stmt_label)?;
      match reagent {
        Reagent::Root => *reagent_has_root = true,
        Reagent::Hex => *reagent_has_hex = true,
        Reagent::Slot(_) => reagent_slots.push(reagent),
      }
      Ok(())
    }
    "create" => {
      // Path is `output.create.<ref-segments>.<location_id>`. The
      // location id is the last segment; everything before it is
      // the card ref.
      if body.len() < 2 {
        return Err(format!(
          "{}: output.create needs `<ref>.<location_id>: <def-key>`",
          stmt_label()
        ));
      }
      let location_seg = body.last().unwrap();
      let location_word = location_seg.as_word().ok_or_else(|| {
        format!("{}: create location id must be a word", stmt_label())
      })?;
      let place = match location_word {
        "inventory" => ProductPlace::Inventory,
        other => {
          return Err(format!(
            "{}: create location {:?} not supported in v1 (only `inventory`)",
            stmt_label(),
            other
          ));
        }
      };
      let ref_segs = &body[..body.len() - 1];
      let owner = parse_create_owner(ref_segs, stmt_label)?;
      let def_key = value
        .and_then(crate::recipe_statement::StatementValue::as_str)
        .ok_or_else(|| format!("{}: create needs a card-def-key string value", stmt_label()))?;
      output_groups.push(ProductGroup {
        target: ProductTarget { place, owner },
        entities: vec![Entity::Card(def_key.to_string())],
      });
      Ok(())
    }
    "modify" => {
      let (aspect, op) = parse_modify_target(body, stmt_label)?;
      let delta_i64 = value
        .and_then(crate::recipe_statement::StatementValue::as_int)
        .ok_or_else(|| format!("{}: modify needs an integer value", stmt_label()))?;
      if delta_i64 < 0 {
        return Err(format!(
          "{}: modify value {} is negative (use the `sub` op instead)",
          stmt_label(),
          delta_i64
        ));
      }
      let delta = delta_i64 as u8;
      let max_for_op = match op {
        StockOp::Add | StockOp::Sub => CONSUME_DELTA_MAX,
        // `set` clamps at runtime to the slot's `max`; the parser
        // checks the u2 ceiling — anything beyond is unrepresentable
        // regardless of the destination slot's declared max.
        StockOp::Set => CONSUME_DELTA_MAX,
      };
      if delta_i64 > max_for_op as i64 || delta_i64 < 1 && !matches!(op, StockOp::Set) {
        return Err(format!(
          "{}: modify delta {} out of range (use 1..={} for add/sub; 0..={} for set)",
          stmt_label(),
          delta_i64,
          max_for_op,
          max_for_op
        ));
      }
      // Predicate-strength gate (preserved from `parse_consume_block`):
      // a `sub` on the same aspect the `hex` entity predicates on
      // must not undershoot its `min`.
      if matches!(op, StockOp::Sub) {
        if let Some(Entity::Aspect(a, min)) = hex_entity {
          if *a == aspect && (*min as i64) < delta_i64 {
            return Err(format!(
              "{}: modify `hex.aspect.{:?}.sub: {}` exceeds hex entity's min {} — \
               the recipe could match a tile whose stock can't satisfy the sub",
              stmt_label(),
              aspect,
              delta_i64,
              min
            ));
          }
        }
      }
      consume_stocks.push(ConsumeStock {
        role: ConsumeRole::Hex,
        aspect_id: aspect,
        op,
        delta,
      });
      Ok(())
    }
    other => Err(format!(
      "{}: unknown output sub-bucket {:?} (expected create / destroy / modify)",
      stmt_label(),
      other
    )),
  }
}

// --- Duration applier ---

#[allow(clippy::too_many_arguments)]
fn apply_duration_statement(
  tail: &[crate::recipe_statement::Segment],
  value: Option<&crate::recipe_statement::StatementValue>,
  _type_ids: &BTreeMap<String, u8>,
  stmt_label: &dyn Fn() -> String,
  default: &mut Option<u32>,
  tiers: &mut Vec<(u32, Entity)>,
) -> Result<(), String> {
  let head = tail
    .first()
    .and_then(crate::recipe_statement::Segment::as_word)
    .ok_or_else(|| format!("{}: duration needs `.default` or `.when.*`", stmt_label()))?;
  match head {
    "default" => {
      if tail.len() != 1 {
        return Err(format!(
          "{}: duration.default takes no further path segments",
          stmt_label()
        ));
      }
      let seconds = value
        .and_then(crate::recipe_statement::StatementValue::as_int)
        .ok_or_else(|| format!("{}: duration.default needs an integer", stmt_label()))?;
      if seconds < 0 {
        return Err(format!("{}: duration cannot be negative", stmt_label()));
      }
      if default.is_some() {
        return Err(format!(
          "{}: duration.default declared more than once",
          stmt_label()
        ));
      }
      *default = Some(seconds as u32);
      Ok(())
    }
    "when" => {
      // Expect: `duration.when.aspect.<name>.min.<N>: <seconds>`
      let body = &tail[1..];
      if body.len() != 4
        || body[0].as_word() != Some("aspect")
        || body[2].as_word() != Some("min")
      {
        return Err(format!(
          "{}: duration.when must be `when.aspect.<name>.min.<N>: <seconds>`",
          stmt_label()
        ));
      }
      let aspect_name = body[1].as_word().ok_or_else(|| {
        format!("{}: aspect name must be a word", stmt_label())
      })?;
      let threshold = body[3].as_index().ok_or_else(|| {
        format!("{}: when.aspect.{}.min.<N>: <N> must be an integer", stmt_label(), aspect_name)
      })?;
      let seconds_i64 = value
        .and_then(crate::recipe_statement::StatementValue::as_int)
        .ok_or_else(|| format!("{}: duration.when needs an integer seconds value", stmt_label()))?;
      if seconds_i64 < 0 {
        return Err(format!("{}: when seconds cannot be negative", stmt_label()));
      }
      let aspect = resolve_aspect_segment(aspect_name, stmt_label)?;
      tiers.push((seconds_i64 as u32, Entity::Aspect(aspect, threshold as i32)));
      Ok(())
    }
    other => Err(format!(
      "{}: unknown duration sub-key {:?} (expected default / when)",
      stmt_label(),
      other
    )),
  }
}

/// Stock-cap mirror of `packed::ZONE_TILE_STOCK_MAX`. Kept local to
/// avoid pulling `packed::` into this module's parser path; deltas
/// above this would never satisfy their own `min` predicate.
const CONSUME_DELTA_MAX: u8 = 3;


#[cfg(test)]
mod tests {
  use super::*;
  use crate::definition_core::find_packed_by_key;

  /// `match_magnetic_recipe` returns `Ok(None)` for a card that has
  /// no `lifecycle_recipe_key` on its def. Sanity check against real
  /// content — picks any non-magnetic card we know exists.
  #[test]
  fn magnetic_returns_none_for_non_magnetic_root() {
    // `axe` is a regular card in the current content registry.
    let axe = find_packed_by_key("axe").expect("registry").expect("axe def");
    let result =
      match_magnetic_recipe(axe, &[], StackDirection::Up, None).expect("matcher");
    assert!(result.is_none(), "non-magnetic card should not match magnetic recipe");
  }

  /// Regular stack matcher accepts an empty `slot_defs` slice without
  /// panicking or erroring. Failure recipes (Phase 7) will rely on
  /// zero-slot matching to fire against a transformed magnetic card
  /// when no slot inputs are required.
  #[test]
  fn stack_matcher_accepts_empty_slots() {
    let axe = find_packed_by_key("axe").expect("registry").expect("axe def");
    // No recipe currently matches `(hex=0, root=axe, slots=[])`, so
    // we expect Ok(0) — "no match" rather than an error.
    let result =
      match_stack_recipe(0, None, axe, &[], StackDirection::Up).expect("matcher");
    assert_eq!(result, 0, "empty slot_defs should yield no-match, not error");
  }

  /// `cut_tree` migrated to the flat-statement shape — verify the
  /// reagent / output / consume / has shapes parse to what
  /// action_completion expects. Catches regressions where a path
  /// segment is silently lost (e.g. the `slot.0` destroy ref not
  /// landing in `reagents.slots`).
  #[test]
  fn cut_tree_flat_recipe_parses_to_expected_shape() {
    let r = find_recipe("cut_tree").expect("registry").expect("cut_tree present");

    // Slot 0 predicates on the corpus+ aspect (stock ≥ 1).
    assert_eq!(r.slots.len(), 1, "cut_tree has one slot");
    let corpus_plus = aspect_id("corpus+")
      .expect("registry")
      .expect("corpus+ registered");
    match &r.slots[0] {
      Entity::Aspect(id, min) => {
        assert_eq!(*id, corpus_plus, "slot[0] aspect should be corpus+");
        assert_eq!(*min, 1, "slot[0] aspect.min == 1");
      }
      other => panic!("slot[0] expected Aspect(corpus+, 1), got {:?}", other),
    }

    // Hex predicate: aspect.wood.min: 1.
    match &r.hex {
      Some(Entity::Aspect(_, min)) => assert_eq!(*min, 1, "hex.aspect.wood.min == 1"),
      other => panic!("hex expected Aspect(_, 1), got {:?}", other),
    }

    // Has predicate: actor's UP-stack contains an axe.
    assert_eq!(r.has.actor.len(), 1, "has.actor has one entry");
    match &r.has.actor[0] {
      Entity::Card(key) => assert_eq!(key, "axe"),
      other => panic!("has.actor[0] expected Card(\"axe\"), got {:?}", other),
    }

    // Reagent slots: slot.0 destroys the corpus actor.
    let has_slot_0 = r.reagents.slots.iter().any(|r| matches!(r, Reagent::Slot(0)));
    assert!(
      has_slot_0,
      "reagents.slots should contain Slot(0) for `output.destroy.slot.0`, got {:?}",
      r.reagents.slots
    );

    // Stock mutation: hex.aspect.wood.sub: 1.
    assert_eq!(r.consume.len(), 1, "cut_tree has one modify clause");
    let m = &r.consume[0];
    assert_eq!(m.op, StockOp::Sub);
    assert_eq!(m.delta, 1);

    // Products: two cards land in the actor's owner's inventory.
    assert_eq!(r.output.len(), 2, "cut_tree spawns two products");
    for group in &r.output {
      assert_eq!(group.target.place, ProductPlace::Inventory);
      assert_eq!(group.target.owner, ProductOwner::Actor);
    }
  }

  /// Sub-aspect hierarchy: `berry` is declared nested under `food`
  /// in `aspects.json`, so `is_aspect_descendant(berry, food)` must
  /// be true. This is what makes a `food` predicate accept a tile
  /// carrying `berry` in its stock — the matcher widening at
  /// `entity_specificity` / `entity_satisfied_with_stocks` sums
  /// values across every descendant of the predicate target.
  #[test]
  fn berry_is_descendant_of_food() {
    let berry = aspect_id("berry").expect("registry").expect("berry registered");
    let food  = aspect_id("food").expect("registry").expect("food registered");
    assert!(
      crate::definition_core::is_aspect_descendant(berry, food).expect("ancestor walk"),
      "berry should descend from food (nested in aspects.json)"
    );
    // Self-case is trivially true.
    assert!(
      crate::definition_core::is_aspect_descendant(food, food).expect("ancestor walk")
    );
    // The reverse direction is false — food is the ancestor, not a
    // descendant of its own child.
    assert!(
      !crate::definition_core::is_aspect_descendant(food, berry).expect("ancestor walk"),
      "food should NOT descend from berry"
    );
    // Cross-family: berry and flora share no ancestor.
    let flora = aspect_id("flora").expect("registry").expect("flora registered");
    assert!(
      !crate::definition_core::is_aspect_descendant(berry, flora).expect("ancestor walk"),
      "berry should NOT descend from flora"
    );
  }

  /// Matcher widening — `entity_specificity` against a def carrying
  /// `berry: 1` should report a non-zero score for a predicate
  /// asking `food, min: 1`. This is the behavior that lets recipes
  /// like "eat food" accept berries without naming them.
  #[test]
  fn food_predicate_matches_berry_def() {
    use crate::definition_core::AspectId;
    let berry: AspectId = aspect_id("berry").unwrap().unwrap();
    let food:  AspectId = aspect_id("food").unwrap().unwrap();
    // Build a fake def carrying berry: 1.
    let def = CardDefinition {
      card_type: 0,
      definition_id: 1,
      key: "test_berry_basket".to_string(),
      style: vec![],
      sprite: None,
      aspects: vec![(berry, 1)],
      traits: Vec::new(),
      flags: 0,
      stock: Vec::new(),
      lifecycle_recipe_key: None,
      lifecycle_duration_ms: None,
    };
    let pred = Entity::Aspect(food, 1);
    let score = entity_specificity(&pred, &def);
    assert!(
      score > 0,
      "Entity::Aspect(food, 1) should match a def carrying berry:1, got score {}",
      score
    );
  }

  /// Stock-aware matcher: a tile def with `pine` in its `stock` array
  /// and a row-stock value of `pine = 2` should satisfy a `wood, min:
  /// 1` predicate. Pine descends from wood in `aspects.json`; the
  /// stock value is the source of truth (no static `aspects` entry on
  /// this def). Exercises the path that makes `cut_tree` match on a
  /// forest tile whose pine stock is non-zero.
  #[test]
  fn wood_predicate_matches_pine_stock() {
    use crate::definition_core::{AspectId, StockSlot};
    let pine: AspectId = aspect_id("pine").unwrap().unwrap();
    let wood: AspectId = aspect_id("wood").unwrap().unwrap();
    let def = CardDefinition {
      card_type: 0,
      definition_id: 1,
      key: "test_pine_forest".to_string(),
      style: vec![],
      sprite: None,
      // No static aspect entry — pine is row-mutable stock only,
      // exactly mirroring forest_1/2/3 in cards/data/tiles/forest.json.
      aspects: Vec::new(),
      traits: Vec::new(),
      flags: 0,
      stock: vec![StockSlot {
        aspect_id: pine,
        max: 3,
        default: 0,
        climate_axis: None,
        climate_axis_min: 0.0,
        climate_axis_max: 1.0,
      }],
      lifecycle_recipe_key: None,
      lifecycle_duration_ms: None,
    };
    let pred = Entity::Aspect(wood, 1);
    // Stocks=None: must not match (no static aspects).
    assert_eq!(
      entity_specificity(&pred, &def),
      0,
      "without stock row data, pine-only def shouldn't satisfy wood predicate",
    );
    // Stocks=(2, 0): pine row stock = 2, should satisfy wood.min:1.
    assert!(
      entity_specificity_with_stocks(&pred, &def, Some((2, 0))) > 0,
      "pine row stock = 2 should satisfy Entity::Aspect(wood, 1)"
    );
    // Stocks=(0, 0): depleted tile must NOT match (depletion semantics).
    assert_eq!(
      entity_specificity_with_stocks(&pred, &def, Some((0, 0))),
      0,
      "pine row stock = 0 must reject wood.min:1 even though pine slot is declared"
    );
  }

  /// Mirror of `wood_predicate_matches_pine_stock` for the direct
  /// (non-widened) case: a `stone` stock slot satisfies `stone.min:1`.
  /// This is the case the user reported as "functioning" — without
  /// sub-aspect widening involvement.
  #[test]
  fn stone_predicate_matches_stone_stock() {
    use crate::definition_core::{AspectId, StockSlot};
    let stone: AspectId = aspect_id("stone").unwrap().unwrap();
    let def = CardDefinition {
      card_type: 0,
      definition_id: 1,
      key: "test_stone_mountain".to_string(),
      style: vec![],
      sprite: None,
      aspects: Vec::new(),
      traits: Vec::new(),
      flags: 0,
      stock: vec![StockSlot {
        aspect_id: stone,
        max: 3,
        default: 0,
        climate_axis: None,
        climate_axis_min: 0.0,
        climate_axis_max: 1.0,
      }],
      lifecycle_recipe_key: None,
      lifecycle_duration_ms: None,
    };
    let pred = Entity::Aspect(stone, 1);
    assert!(
      entity_specificity_with_stocks(&pred, &def, Some((1, 0))) > 0,
      "stone row stock = 1 should satisfy Entity::Aspect(stone, 1)"
    );
  }
}
