//! Recipe registry built from JSON catalogs in `content/recipes/`.
//!
//! Recipe `id` strings (`"woodcutting"`, etc.) are mapped to stable integer
//! ids in `recipes/id.json`, then packed alongside `recipe_type` (3 bits)
//! and `recipe_category` (3 bits) into a `u16` via [`crate::packed::pack_recipe`].
//! The packed value is what `Action.recipe` carries on the wire.
//!
//! Resolves `"@<type>"` entity strings against the card-type registry in
//! [`crate::definition_core`]; aspect-name strings against
//! [`crate::definition_core::aspect_id`]. Both registries are built lazily;
//! a transitive build failure on either side surfaces as the recipe
//! registry's stored error.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::definition_core::{aspect_id, card_type_ids, decode_definition, AspectId, CardDefinition};
use crate::packed::{pack_recipe, RECIPE_ID_MASK, RECIPE_TYPE_OR_CATEGORY_MASK};

// ---------- Recipes ----------

/// A condition tree against a card. Used both to validate slot fillers
/// (where the tree is matched against a candidate card) and to drive
/// product generation (where `WeightedOr` selects between two outputs).
///
/// JSON grammar:
/// - `"corpus"` → `Card("corpus")` — match a card with this exact key
/// - `"any"` → `Any` — match any card (lowest specificity)
/// - `"@discipline"` → `Type(type_id)` — match any card whose `card_type`
///   resolves to the named type. Resolved at recipe-registry build time
///   against `cards/types.json`; an unknown type is a build error.
/// - `["aspect", N]` → `Aspect(aspect_id("aspect"), N)` — match a card
///   whose aspect value is ≥ N
/// - `[E]` → just `E` (degenerate one-element array, common in slot
///   wrapping)
/// - `[E1, E2]` → `And(E1, E2)`
/// - `[E1, [], E2]` → `Or(E1, E2)`
/// - `[E1, [Wa, Wb], E2]` → `WeightedOr(E1, E2, Wa, Wb)` (intended for
///   products; treated as a non-weighted `Or` if used inside a slot match)
///
/// # Match specificity (used by the priority weighting in `actions.rs`)
///
/// When an entity matches a card, the per-leaf weight (more specific →
/// higher) is:
///
/// - `Card`: 4
/// - `Aspect`: 3
/// - `Type`: 2
/// - `Any`: 1
///
/// For composite entities, `And` sums the children's weights (slot is
/// more specific than either alone), `Or` / `WeightedOr` take the weight
/// of whichever branch satisfied (or 0 if neither did).
#[derive(Debug, Clone)]
pub enum Entity {
  Card(String),
  Aspect(AspectId, i32),
  /// Match any card whose `card_type` equals this `u8`. Resolved at
  /// recipe-build time so the matcher doesn't need a registry lookup
  /// per check.
  Type(u8),
  /// Match any card. Lowest specificity — used as a slot wildcard.
  Any,
  And(Box<Entity>, Box<Entity>),
  Or(Box<Entity>, Box<Entity>),
  WeightedOr {
    a: Box<Entity>,
    b: Box<Entity>,
    weight_a: u32,
    weight_b: u32,
  },
}

/// Determines what shape of trigger fires the recipe.
///
/// - `Stack(Up)` / `Stack(Down)` — fired when the client submits a
///   stack via `submit_inventory_stacks`; the server tries to fit the
///   slots along the up- or down-branch from the submitted root.
/// - `OnCreate` — fired when a card is inserted via `insert_card_row`;
///   the new card itself is checked against the recipe's `hex`
///   and/or `root` entity. At least one of those two must be set
///   (parser-enforced): `hex` requires the new card to be a hex-shaped
///   type matching the entity, `root` matches any type. An `OnCreate`
///   recipe with a non-`None` `magnetic` field doubles as a magnetic
///   recipe — the matched action installs the slot-fill ticker (see
///   `magnetic.rs`) on the new card, and the inner recipes describe
///   what the server pulls from the player's inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeType {
  Stack(StackDirection),
  OnCreate,
}

/// Which way a stack recipe walks the chain from the submitted root.
/// `Up` matches what the JSON schema calls the `up` direction (the
/// player has stacked cards "above" the root); `Down` matches `down`.
/// The pair of values mirrors `InventoryStack { stack_up, stack_down }`
/// on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
  Up,
  Down,
}

/// Magnetic recipes nest a bucket-style sub-tree of *inner* recipes
/// inside the outer recipe's `magnetic` field. The outer is a normal
/// recipe (matched against the chain like anything else); the inner
/// recipes describe what cards the magnetic phase pulls from the
/// player's inventory and stacks onto the magnetic action's anchor.
///
/// Schema:
///
/// ```json
/// "magnetic": {
///   "type": "stack",
///   "up":   [ {inner}, {inner}, … ],
///   "down": [ {inner}, … ]
/// }
/// ```
///
/// At parse time we flatten the directional arrays into a single
/// `inners` list, baking each inner's direction into its
/// `recipe_type`. The flat order is the same order the parser walked,
/// which is the index used for the *sub-id* stored in the queued
/// inner action's `flags` (high 4 bits — capped at 16 inners per
/// outer).
#[derive(Debug, Clone)]
pub struct MagneticBucket {
  pub inners: Vec<InnerRecipe>,
}

/// One inner recipe inside a [`MagneticBucket`]. Like a top-level
/// recipe but without `id` (sub-identified by its position in
/// `MagneticBucket.inners`), without nested `magnetic` (the design
/// doesn't recurse — magnetic recipes can't themselves contain
/// magnetic recipes), and without `interval` (only the outer carries
/// the magnetic phase cadence).
///
/// `recipe_type` is baked in from the bucket's direction key at parse
/// time: under `"up"` it's `Stack(Up)`, under `"down"` it's
/// `Stack(Down)`, under `"self"` it's `OnCreate`. The tick uses this
/// to know which way to walk the chain when matching slot fillers.
///
/// `duration` is the inner action's duration (in seconds) once the
/// magnetic phase queues it into `actions` — distinct from the outer
/// recipe's `duration`, which is the magnetic-phase loop-count cap.
#[derive(Debug, Clone)]
pub struct InnerRecipe {
  pub recipe_type: RecipeType,
  pub root: Option<Entity>,
  pub hex: Option<Entity>,
  pub slots: Vec<Entity>,
  pub reagents: Vec<Reagent>,
  pub output_success: Vec<ProductGroup>,
  pub duration: Duration,
  /// Flags to set on pulled cards when this inner's magnetic phase
  /// acquires them. See [`SetStartFlags`].
  pub set_start: SetStartFlags,
}

/// Where the output cards from a completed action go. Two independent
/// axes:
///
/// - `place` — what *kind* of destination (an inventory panel today;
///   future: a hex tile, a loose world spot, a player's pile, …).
/// - `owner` — which referent of the action defines that destination
///   (the chain root, the actor, future: the underlying tile, …).
///
/// The recipe JSON encodes this as a nested map under
/// `output_success` (the action completed normally) or `output_fail`
/// (a magnetic outer ran out of loop budget without queueing an
/// inner):
///
/// ```json
/// "output_success": {
///   "inventory": {
///     "root":  [/* entities */],
///     "actor": [/* entities */]
///   }
/// }
/// ```
///
/// Outer key picks the [`ProductPlace`], inner key picks the
/// [`ProductOwner`]. Adding a new place (e.g. `"hex"`) or a new owner
/// (e.g. `"hex"` to mean "the tile under the action") is one enum
/// variant + one match arm in `actions::resolve_product_destination`,
/// not a new flat target name per combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductTarget {
  pub place: ProductPlace,
  pub owner: ProductOwner,
}

/// What *kind* of destination a product lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductPlace {
  /// A player's inventory panel. Combined with [`ProductOwner`] to
  /// pick *which* player's panel. Today this is the only supported
  /// place; world-tile and loose-world placements will land alongside
  /// the world board.
  Inventory,
}

/// Which action-relative referent the product is attached to. Each
/// variant resolves to a player_id (the panel owner) at completion
/// time; the destination is always the inventory at `LAYER_INVENTORY`.
///
/// JSON keys: `"root"`, `"actor"`, `"hex"`, `"action"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductOwner {
  /// Chain root's owner. For `OnCreate` (root == actor), resolves to
  /// the actor's `owner_id`. For stack recipes the chain root isn't
  /// held by the action and isn't recoverable from server state at
  /// completion, so this currently falls back to the action owner —
  /// distinct from `Actor` only when a future change persists the
  /// root id on `ActionScheduler`.
  Root,
  /// Actor card's owner — `Card.owner_id` of `action.card_id`.
  Actor,
  /// Hex card's owner. The matcher persists the resolved hex id on
  /// `ActionScheduler.hex_card_id` at start time; completion looks
  /// it up and reads `hex_card.owner_id`. Falls back to the action
  /// owner when the chain isn't on a hex, the hex is unowned
  /// (`owner_id == 0`), or the hex resolved from a `Zone` cell
  /// (which doesn't carry an `owner_id`).
  Hex,
  /// Action's owner — `Action.owner_id`, set by `start_action`.
  /// Always present; the most reliable fallback when the
  /// card-relative owners can't be resolved.
  Action,
}

#[derive(Debug, Clone)]
pub struct ProductGroup {
  pub target: ProductTarget,
  /// Each entity in this list produces one output card on completion.
  /// `WeightedOr` entities pick one alternative at random.
  pub entities: Vec<Entity>,
}

/// Flags to set on specific cards when a magnetic action pulls them
/// at the start of an inner recipe, or on the anchor card when a
/// magnetic action is first installed. Field values are bitmasks built
/// from the named-flag arrays in the JSON `"set_start"` key. Zero
/// means "set nothing on that card role."
///
/// The clearing counterpart is always `position_hold + drop_hold`
/// (the two temporary-hold bits) and is not controlled by this struct.
/// Locked variants (`position_locked`, `drop_locked`) set via
/// `set_start` persist until explicitly cleared by admin/migration code.
///
/// JSON form under an inner recipe:
/// ```json
/// "set_start": {
///   "root": ["position_hold", "drop_hold"],
///   "slot": ["position_hold", "drop_hold"],
///   "hex":  []
/// }
/// ```
/// Under an outer recipe the `hex` key sets flags on the anchor card at
/// install time; `root` / `slot` are parsed but unused at that point.
#[derive(Debug, Clone, Default)]
pub struct SetStartFlags {
  /// Bitmask ORed onto the root card (root-based inners: the card
  /// placed as a child of the anchor; outer recipe: the anchor).
  pub root: u8,
  /// Bitmask ORed onto each slot card (slot-based inners: cards pulled
  /// from inventory into numbered slot positions).
  pub slot: u8,
  /// Bitmask ORed onto the hex anchor card (outer recipe: applied at
  /// install; inner recipe: parsed but not yet applied).
  pub hex: u8,
}

/// What a recipe consumes on completion. Three kinds, all optional:
///
/// - `Root` — the chain root card. For stack recipes that's the
///   submitted root (which isn't held by a `CardHold` today, so this
///   is a no-op for stack types until chain context-at-completion
///   lands). For `OnCreate`, root and actor are the same card, so
///   this resolves to `action.card_id`.
/// - `Hex` — the hex card the action is anchored to (recorded on
///   `ActionScheduler.hex_card_id` at start time).
/// - `Slot(N)` — the 1-indexed slot position. Slot 1 is always the
///   actor (`action.card_id`); slot 2+ requires per-slot claim
///   tracking that doesn't exist yet.
///
/// JSON form: strings `"root"` and `"hex"` for the named referents,
/// integers `1..=255` for slot positions. Integer `0` is no longer
/// accepted — use `"root"` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reagent {
  Root,
  Hex,
  Slot(u8),
}

/// Recipe duration. Either fixed seconds or a list of `(seconds,
/// condition)` cases evaluated against an aspect pool, with a fallback at
/// the tail.
#[derive(Debug, Clone)]
pub enum Duration {
  Fixed(u32),
  Conditional {
    cases: Vec<(u32, Entity)>,
    fallback: u32,
  },
}

#[derive(Debug, Clone)]
pub struct RecipeDef {
  /// Packed stable ID. Layout (see [`crate::packed::pack_recipe`]):
  /// `[recipe_type:u3][recipe_category:u3][recipe_id:u10]`. The
  /// `recipe_id` (low 10 bits) comes from `recipes/id.json`; the
  /// `recipe_type` and `recipe_category` (high 6 bits) come from
  /// `recipes/types.json` and identify the bucket the recipe was
  /// declared under. Stored in `Action.recipe` and
  /// `MagneticAction.recipe` on the wire; never reassigned — safe
  /// across recipe additions and reorders.
  pub index: u16,
  /// Human-readable id from JSON, e.g. `"woodcutting"`.
  pub id: String,
  pub recipe_type: RecipeType,
  /// For `OnCreate`: when set, the new card itself must satisfy this
  /// entity (no shape constraint). At least one of `root` / `hex`
  /// must be set for `OnCreate`. For stack types this is `None`
  /// unless the recipe wants to constrain the chain root separately
  /// from the slot list.
  pub root: Option<Entity>,
  /// Optional hex tier. Semantics depend on `recipe_type`:
  ///
  /// - **Stack**: a condition on the hex card the chain root is
  ///   attached to. A rectangle root with `stacked_state == 3` carries
  ///   the hex card's id in `micro_location`; the matcher resolves
  ///   that and scores this entity against the hex card's definition.
  /// - **OnCreate**: a condition on the *new card itself* — it must
  ///   be a hex-shaped type ([`is_hex_type`] returns `true`) and its
  ///   def must satisfy the entity. Matching here installs the
  ///   action / magnetic_action with the new card as the anchor.
  ///
  /// When `None`, the hex tier contributes 0. When `Some(_)`, it's
  /// the top of the priority hierarchy — a satisfied `hex` outranks
  /// any combination of `root` and `slots` weights.
  pub hex: Option<Entity>,
  /// Slot list. For `Stack(_)` recipes, slot 1 is the actor; slots 2..
  /// fill in chain order from the actor outward along the recipe's
  /// branch direction. Empty for non-magnetic `OnCreate`. **For
  /// magnetic recipes** (`magnetic.is_some()`), the slots describe the
  /// inputs the server pulls from the player's inventory — the actor
  /// is *not* in this list — and `slots[0]` is the first magnetic
  /// input, stacked on the actor (or attached as a hex root if the
  /// actor is hex-shaped) and so on.
  pub slots: Vec<Entity>,
  /// What the recipe consumes on completion. See [`Reagent`] —
  /// strings `"root"` / `"hex"` and 1-indexed slot integers in JSON.
  pub reagents: Vec<Reagent>,
  /// Cards produced when the recipe's action completes normally —
  /// the inner queued for a magnetic outer, the standard end-of-action
  /// fire for everything else. JSON key: `output_success`.
  pub output_success: Vec<ProductGroup>,
  /// Cards produced when a magnetic outer's loop budget runs out
  /// without ever queueing an inner. JSON key: `output_fail`. Empty
  /// for non-magnetic recipes (no failure path).
  pub output_failure: Vec<ProductGroup>,
  /// Action duration in seconds. Optional **only** for outer magnetic
  /// recipes (where `magnetic.is_some()`), and there it acts as the
  /// magnetic-phase loop-count cap (in ticks, not seconds). For
  /// non-magnetic recipes this is the seconds-from-start the action
  /// runs in `actions` before completion fires; the parser requires
  /// it for those. `None` on a magnetic outer means "no terminator —
  /// magnetic action runs until it queues an inner or is cancelled".
  pub duration: Option<Duration>,
  /// When set, this recipe is *magnetic*: `magnetic.rs` installs a
  /// scheduled tick that pulls inventory cards into the action's
  /// chain per the bucket's inner recipes, then queues an inner
  /// action into `actions` once any inner's slot list is fully
  /// filled. The outer recipe's `slots` / `reagents` /
  /// `output_success` / `output_failure` describe the magnetic
  /// *outer*: matched at install time, fired
  /// at magnetic-action completion — `output_success` fires when an
  /// inner gets queued, `output_failure` fires when the loop cap is
  /// reached without queueing one. The inner recipe's fields fire at
  /// the queued inner action's completion.
  pub magnetic: Option<MagneticBucket>,
  /// Tick cadence in seconds for the magnetic phase. Only meaningful
  /// when `magnetic.is_some()`; ignored otherwise. The magnetic_action
  /// schedules a tick every `interval` seconds; each tick attempts
  /// one card pickup. Required for magnetic recipes.
  pub interval: Option<u32>,
  /// Flags to set on cards at the start of the action. For magnetic
  /// recipes, `hex` is applied to the anchor card when `magnetic::install`
  /// runs; `root` / `slot` are reserved for future outer-recipe use.
  /// For non-magnetic recipes this struct is currently unused (all zeros).
  /// See [`SetStartFlags`].
  pub set_start: SetStartFlags,
}

const RECIPES_FILES: &[(&str, &str)] = &[
  ("recipes/data/01.json", include_str!("../recipes/data/01.json")),
];
const RECIPE_IDS_JSON: &str = include_str!("../recipes/id.json");
const RECIPE_TYPES_JSON: &str = include_str!("../recipes/types.json");

struct RecipeRegistry {
  /// Packed stable ID → recipe definition. Key is the same `u16`
  /// `Action.recipe` carries on the wire (see
  /// [`crate::packed::pack_recipe`]).
  by_id: BTreeMap<u16, RecipeDef>,
  /// Human-readable name → packed stable ID.
  id_by_name: BTreeMap<String, u16>,
  /// `(type_id, category_id)` → packed stable IDs in declaration order.
  by_type: BTreeMap<(u8, u8), Vec<u16>>,
}

static RECIPES: OnceLock<Result<RecipeRegistry, String>> = OnceLock::new();

fn recipes_registry() -> Result<&'static RecipeRegistry, String> {
  RECIPES.get_or_init(build_recipes).as_ref().map_err(|e| e.clone())
}

/// Look up a recipe by its packed stable ID (what `Action.recipe`
/// stores). Returns `Ok(None)` if no recipe with that ID is registered.
pub fn recipe(index: u16) -> Result<Option<&'static RecipeDef>, String> {
  Ok(recipes_registry()?.by_id.get(&index))
}

/// Look up a recipe by its human-readable id. `Ok(None)` if unknown.
pub fn find_recipe(id: &str) -> Result<Option<&'static RecipeDef>, String> {
  let registry = recipes_registry()?;
  let Some(&stable_id) = registry.id_by_name.get(id) else {
    return Ok(None);
  };
  Ok(registry.by_id.get(&stable_id))
}

/// All recipes of a given type, in declaration order.
pub fn recipes_of_type(rt: RecipeType) -> Result<Vec<&'static RecipeDef>, String> {
  let registry = recipes_registry()?;
  let key = recipe_type_pair(rt)?;
  let Some(ids) = registry.by_type.get(&key) else {
    return Ok(Vec::new());
  };
  Ok(ids.iter().filter_map(|id| registry.by_id.get(id)).collect())
}

/// Detailed result of a successful stack-recipe match: which recipe,
/// where the actor slot window aligned in the chain, and which optional
/// fields the recipe carried. Used by clients that need to know
/// alignment to call the `propose_action` reducer with the correct
/// `slots` slice and root/hex args.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "js", derive(serde::Serialize))]
#[cfg_attr(feature = "js", serde(rename_all = "camelCase"))]
pub struct StackMatch {
  /// Stable packed id of the matched recipe. Same value the simple
  /// `match_stack_recipe` returns.
  pub recipe_index: u16,
  /// Index into the full chain (`[root] ++ slot_defs`) where the
  /// recipe's slot window starts. `0` means the actor binds to the
  /// chain root; `1` means the recipe.root tier consumed `chain[0]`
  /// (or the matcher's sliding picked a later position).
  pub slot_start: u32,
  /// Number of cards in the recipe's slot window
  /// (= `recipe.slots.len()`). The chain slice
  /// `chain[slot_start..slot_start + slot_count]` is what fills the
  /// recipe's slot list.
  pub slot_count: u32,
  /// Whether the matched recipe carries a `root` constraint. Drives
  /// what the client passes for the `propose_action` `root` arg
  /// (`chain[0]` if `true`, `0` if `false` so the server's flag
  /// machinery treats the action as free-floating).
  pub has_root: bool,
  /// Whether the matched recipe carries a `hex` constraint. Mirrors
  /// `has_root` for the `hex` arg.
  pub has_hex: bool,
}

/// Find the highest-priority `Stack(direction)` recipe whose conditions
/// are satisfied by the given chain, and return its packed id. Returns
/// `Ok(0)` when no recipe matches (`0` is unreachable as a real packed
/// id — `recipes/id.json` rejects raw id `0`).
///
/// Inputs are `u16` `packed_definition`s (see
/// [`crate::definition_core::decode_definition`]). Pass `0` for
/// `hex_def` when the chain isn't anchored to a hex; pass `0` for
/// `root_def` if the caller has no chain root to bind separately from
/// the slot list. The full chain considered for matching is
/// `[root_def] ++ slot_defs` (root first, then the cards stacked
/// outward in `direction`).
///
/// # Eligibility & actor sliding
///
/// A recipe is eligible iff every condition it declares is satisfied:
/// - if `recipe.hex` is `Some`, the def at `hex_def` must satisfy it;
/// - if `recipe.root` is `Some`, the def at `root_def` (= `chain[0]`)
///   must satisfy it;
/// - the recipe's `slots` window can be aligned at some start index
///   `s` along the chain such that `chain[s + i]` satisfies
///   `recipe.slots[i]` for every `i`. The minimum `s` is `1` when
///   `recipe.root` is set (the root tier is consumed separately) and
///   `0` otherwise (the actor — `slots[0]` — may bind to the root
///   itself). The maximum `s` is `chain.len() - recipe.slots.len()`.
///
/// When several start positions match, the highest slot-specificity
/// sum wins for that recipe.
///
/// # Ranking
///
/// Highest-first lexicographic tuple:
/// 1. Hex leaf specificity
/// 2. Root leaf specificity
/// 3. Best slot leaf-specificity sum across actor-window positions
///
/// Leaf weights: `Card` 4, `Aspect` 3, `Type` 2, `Any` 1; an absent
/// recipe field contributes 0. `And` sums its children's satisfied
/// weights; `Or` / `WeightedOr` take the higher of the two branches
/// (`WeightedOr` weights only steer product selection at completion
/// time, never the match score).
///
/// Tiebreak across recipes with identical scores: the lower packed
/// stable id wins (= earlier declaration order in the recipe JSON).
pub fn match_stack_recipe(
  hex_def: u16,
  root_def: u16,
  slot_defs: &[u16],
  direction: StackDirection,
) -> Result<u16, String> {
  Ok(
    match_stack_recipe_detail(hex_def, root_def, slot_defs, direction)?
      .map(|m| m.recipe_index)
      .unwrap_or(0),
  )
}

/// Same eligibility / ranking as [`match_stack_recipe`], but also
/// returns *where* the actor slot window aligned in the chain and
/// whether the matched recipe carries `root` / `hex` constraints. See
/// [`StackMatch`] for the field semantics. Returns `Ok(None)` when no
/// recipe matched.
pub fn match_stack_recipe_detail(
  hex_def: u16,
  root_def: u16,
  slot_defs: &[u16],
  direction: StackDirection,
) -> Result<Option<StackMatch>, String> {
  let candidates = recipes_of_type(RecipeType::Stack(direction))?;

  let hex_card = decode_definition(hex_def)?;
  let root_card = decode_definition(root_def)?;
  let slot_cards: Vec<Option<&CardDefinition>> = slot_defs
    .iter()
    .map(|&d| decode_definition(d))
    .collect::<Result<_, _>>()?;

  // Full chain: chain[0] = root, chain[1..] = stacked cards in
  // direction order. The actor's slot window walks across this.
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

    // Actor slot window: when `recipe.root` is set, the root tier is
    // consumed at chain[0], so the actor (slots[0]) can start no
    // earlier than chain[1]. When unset, the actor may bind to the
    // root itself (chain[0]) and the window slides outward from there.
    let min_start: usize = if recipe.root.is_some() { 1 } else { 0 };
    if chain.len() < min_start + recipe.slots.len() {
      continue 'recipes;
    }
    let max_start: usize = chain.len() - recipe.slots.len();

    let mut best_for_recipe: Option<(u32, u32)> = None; // (slot_spec, start)
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
/// no match. See [`match_stack_recipe`] for weight conventions.
fn entity_specificity(entity: &Entity, def: &CardDefinition) -> u32 {
  match entity {
    Entity::Card(key) => {
      if &def.key == key {
        4
      } else {
        0
      }
    }
    Entity::Aspect(aspect, min) => {
      let val = def
        .aspects
        .iter()
        .find_map(|(a, v)| (a == aspect).then_some(*v))
        .unwrap_or(0);
      if val >= *min {
        3
      } else {
        0
      }
    }
    Entity::Type(type_id) => {
      if def.card_type == *type_id {
        2
      } else {
        0
      }
    }
    Entity::Any => 1,
    Entity::And(a, b) => {
      let sa = entity_specificity(a, def);
      let sb = entity_specificity(b, def);
      if sa > 0 && sb > 0 {
        sa + sb
      } else {
        0
      }
    }
    Entity::Or(a, b) | Entity::WeightedOr { a, b, .. } => {
      entity_specificity(a, def).max(entity_specificity(b, def))
    }
  }
}

/// Resolve a `RecipeType` variant to its `(type_id, category_id)` pair
/// from `recipes/types.json`. The pair is what `pack_recipe` puts in
/// the high 6 bits of a packed recipe id.
fn recipe_type_pair(rt: RecipeType) -> Result<(u8, u8), String> {
  let registry = recipe_types_registry()?;
  let (type_name, category_name) = recipe_type_names(rt);
  let &type_id = registry.types.get(type_name).ok_or_else(|| {
    format!("recipes/types.json: type {:?} missing — required by RecipeType variant", type_name)
  })?;
  let &category_id = registry.categories.get(category_name).ok_or_else(|| {
    format!(
      "recipes/types.json: category {:?} missing — required by RecipeType variant",
      category_name
    )
  })?;
  Ok((type_id, category_id))
}

/// JSON-side names of a `RecipeType` variant — the bucket type and
/// direction key it was declared under.
fn recipe_type_names(rt: RecipeType) -> (&'static str, &'static str) {
  match rt {
    RecipeType::Stack(StackDirection::Up) => ("stack", "up"),
    RecipeType::Stack(StackDirection::Down) => ("stack", "down"),
    RecipeType::OnCreate => ("on_create", "self"),
  }
}

// ---------- Recipe types registry ----------

struct RecipeTypeRegistry {
  /// `name → recipe_type_id` (3 bits, from `recipes/types.json`'s
  /// `types` section).
  types: BTreeMap<String, u8>,
  /// `name → recipe_category_id` (3 bits, from `recipes/types.json`'s
  /// `categories` section).
  categories: BTreeMap<String, u8>,
}

static RECIPE_TYPES: OnceLock<Result<RecipeTypeRegistry, String>> = OnceLock::new();

fn recipe_types_registry() -> Result<&'static RecipeTypeRegistry, String> {
  RECIPE_TYPES
    .get_or_init(build_recipe_types)
    .as_ref()
    .map_err(|e| e.clone())
}

fn build_recipe_types() -> Result<RecipeTypeRegistry, String> {
  let root: Value = serde_json::from_str(RECIPE_TYPES_JSON)
    .map_err(|e| format!("recipes/types.json: parse failed: {}", e))?;
  let types = recipe_id_section(&root, "types")?;
  let categories = recipe_id_section(&root, "categories")?;
  Ok(RecipeTypeRegistry { types, categories })
}

/// Read a `name → id` map from one section of `recipes/types.json`.
/// Skips reserved/comment keys (those starting with `_`); requires
/// real entries to carry an integer `id` field that fits in 3 bits.
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

fn build_recipes() -> Result<RecipeRegistry, String> {
  // Build the type+category id map first — we need it to pack each
  // recipe's stable ID and to validate that a bucket's type/direction
  // is actually declared in `recipes/types.json`.
  let type_registry = recipe_types_registry()?;

  // Walk `recipes/id.json` (`{ "<type>": { "<category>": { "<key>":
  // <id>, … }, … }, … }`) and flatten it into a single `name →
  // packed_u16` map. The packed value is what `Action.recipe` carries
  // on the wire and what we'll store in `RecipeDef.index`.
  let ids_root: Value = serde_json::from_str(RECIPE_IDS_JSON)
    .map_err(|e| format!("recipes/id.json: parse failed: {}", e))?;
  let ids_obj = ids_root
    .as_object()
    .ok_or_else(|| "recipes/id.json: top-level not an object".to_string())?;

  let mut packed_ids: BTreeMap<String, u16> = BTreeMap::new();
  for (type_name, type_val) in ids_obj {
    let &type_id = type_registry.types.get(type_name).ok_or_else(|| {
      format!(
        "recipes/id.json: type {:?} not declared in recipes/types.json",
        type_name
      )
    })?;
    let type_obj = type_val.as_object().ok_or_else(|| {
      format!("recipes/id.json: entry for type {:?} not an object", type_name)
    })?;
    for (category_name, cat_val) in type_obj {
      let &category_id = type_registry.categories.get(category_name).ok_or_else(|| {
        format!(
          "recipes/id.json: category {:?} (under type {:?}) not declared in recipes/types.json",
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

  // Pull `type_ids` from the cards registry — used by `parse_entity` to
  // resolve `"@<type_name>"` strings into `Entity::Type(<u8>)` at parse
  // time. This drives a transitive build of the card registry; if that
  // fails, recipe build fails too.
  let type_ids = card_type_ids()?.clone();

  let mut by_id: BTreeMap<u16, RecipeDef> = BTreeMap::new();
  let mut id_by_name: BTreeMap<String, u16> = BTreeMap::new();
  let mut by_type: BTreeMap<(u8, u8), Vec<u16>> = BTreeMap::new();

  for (filename, content) in RECIPES_FILES {
    let buckets_value: Value = serde_json::from_str(content)
      .map_err(|e| format!("{}: parse failed: {}", filename, e))?;
    let buckets = buckets_value
      .as_array()
      .ok_or_else(|| format!("{}: top-level not an array of buckets", filename))?;

    for bucket in buckets {
      let bucket_type = bucket
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{}: bucket missing 'type'", filename))?;

      // Each bucket maps `type` to one or more direction-keyed arrays
      // of recipes. The pairs below say "for each direction key the
      // bucket's type allows, find the recipe array under that key
      // and tag its entries with this RecipeType."
      let direction_keys: &[(&str, RecipeType)] = match bucket_type {
        "stack" => &[
          ("up", RecipeType::Stack(StackDirection::Up)),
          ("down", RecipeType::Stack(StackDirection::Down)),
        ],
        "on_create" => &[("self", RecipeType::OnCreate)],
        other => {
          return Err(format!(
            "{}: bucket has unknown type {:?}, expected \"stack\" or \"on_create\"",
            filename, other,
          ));
        }
      };

      for &(direction_key, recipe_type) in direction_keys {
        let Some(arr) = bucket.get(direction_key).and_then(Value::as_array) else {
          continue;
        };
        let pair = recipe_type_pair(recipe_type)?;
        for recipe_value in arr {
          let (id, stable_id, def) = parse_recipe(
            recipe_value,
            recipe_type,
            filename,
            &type_ids,
            &packed_ids,
          )?;
          if id_by_name.contains_key(&id) {
            return Err(format!(
              "{}: recipe id {:?} declared more than once",
              filename, id
            ));
          }
          by_type.entry(pair).or_default().push(stable_id);
          id_by_name.insert(id, stable_id);
          by_id.insert(stable_id, def);
        }
      }
    }
  }

  Ok(RecipeRegistry { by_id, id_by_name, by_type })
}

/// Parse one recipe record from inside a direction-keyed bucket array
/// (`up` / `down` for `stack`, `self` for `on_create`). The
/// surrounding bucket has already supplied the `recipe_type`; the
/// record itself no longer carries a `type` field. Returns
/// `(id, stable_id, def)` for the caller to register. `stable_ids` is
/// the packed-u16 map built by `build_recipes` from the nested
/// `recipes/id.json` (see [`pack_recipe`] for the layout).
fn parse_recipe(
  recipe_value: &Value,
  recipe_type: RecipeType,
  filename: &str,
  type_ids: &BTreeMap<String, u8>,
  stable_ids: &BTreeMap<String, u16>,
) -> Result<(String, u16, RecipeDef), String> {
  let id = recipe_value["id"]
    .as_str()
    .ok_or_else(|| format!("{}: recipe missing 'id'", filename))?
    .to_string();

  let stable_id = stable_ids.get(&id).copied().ok_or_else(|| {
    format!(
      "{}: recipe {:?} not found in recipes/id.json — run gen-ids.py",
      filename, id
    )
  })?;

  // `magnetic` is a nested bucket-style sub-tree. Same shape as the
  // top-level recipe file (a `type` plus direction-keyed inner-recipe
  // arrays), parsed into a flat `MagneticBucket.inners` list with each
  // inner's `recipe_type` baked from the bucket's direction key.
  let magnetic = if let Some(mag_value) = recipe_value.get("magnetic") {
    let mag_obj = mag_value.as_object().ok_or_else(|| {
      format!(
        "{}: recipe {:?} 'magnetic' not an object",
        filename, id
      )
    })?;
    Some(parse_magnetic_bucket(mag_obj, filename, &id, type_ids)?)
  } else {
    None
  };

  // `interval` (seconds) — required when `magnetic.is_some()`, ignored
  // otherwise. Drives the magnetic_action's recurring schedule.
  let interval = match recipe_value.get("interval") {
    Some(v) => {
      let n = v.as_u64().ok_or_else(|| {
        format!(
          "{}: recipe {:?} 'interval' not a non-negative integer: {:?}",
          filename, id, v
        )
      })?;
      Some(u32::try_from(n).map_err(|_| {
        format!(
          "{}: recipe {:?} 'interval' value {} exceeds u32 range",
          filename, id, n
        )
      })?)
    }
    None => None,
  };
  if magnetic.is_some() && interval.is_none() {
    return Err(format!(
      "{}: magnetic recipe {:?} missing required 'interval' field",
      filename, id
    ));
  }
  if magnetic.is_none() && interval.is_some() {
    return Err(format!(
      "{}: non-magnetic recipe {:?} has 'interval' field with no 'magnetic' to consume it",
      filename, id
    ));
  }

  let root = if recipe_value.get("root").is_some() {
    Some(parse_entity(&recipe_value["root"], type_ids, filename, &id, "root")?)
  } else {
    None
  };

  let hex = if recipe_value.get("hex").is_some() {
    Some(parse_entity(&recipe_value["hex"], type_ids, filename, &id, "hex")?)
  } else {
    None
  };

  let slots = if let Some(slots_arr) = recipe_value.get("slots").and_then(Value::as_array) {
    slots_arr
      .iter()
      .enumerate()
      .map(|(i, v)| parse_entity(v, type_ids, filename, &id, &format!("slots[{}]", i)))
      .collect::<Result<Vec<_>, _>>()?
  } else {
    Vec::new()
  };

  let reagents = if let Some(arr) = recipe_value.get("reagents").and_then(Value::as_array) {
    arr
      .iter()
      .map(|v| parse_reagent(v, filename, &id))
      .collect::<Result<Vec<_>, _>>()?
  } else {
    Vec::new()
  };

  // `products` was renamed to `output_success`; fail loud on the
  // legacy key so stale JSON doesn't silently drop its outputs.
  if recipe_value.get("products").is_some() {
    return Err(format!(
      "{}: recipe {:?} uses legacy 'products' key — rename to 'output_success'",
      filename, id
    ));
  }
  let output_success = parse_output_groups(
    recipe_value,
    "output_success",
    type_ids,
    filename,
    &id,
    "",
  )?;
  let output_failure = parse_output_groups(
    recipe_value,
    "output_fail",
    type_ids,
    filename,
    &id,
    "",
  )?;
  if !output_failure.is_empty() && magnetic.is_none() {
    return Err(format!(
      "{}: recipe {:?} has 'output_fail' but no 'magnetic' field — \
       output_fail only fires when a magnetic outer's loop cap is reached",
      filename, id
    ));
  }

  // Duration is optional only for outer magnetic recipes (where it
  // acts as the magnetic-phase loop-count cap; absent means "no
  // terminator"). For everything else the recipe's action runs in
  // `actions` for `duration` seconds, so it's required.
  let duration = if recipe_value.get("duration").is_some() {
    Some(parse_duration(&recipe_value["duration"], type_ids, filename, &id)?)
  } else {
    None
  };
  if duration.is_none() && magnetic.is_none() {
    return Err(format!(
      "{}: non-magnetic recipe {:?} missing required 'duration' field",
      filename, id
    ));
  }

  // OnCreate recipes match against the new card's def via either
  // `hex` (must be a hex-shaped card matching the entity) or `root`
  // (any card type matching the entity). At least one is required —
  // an OnCreate recipe with neither has no way to identify what it
  // fires on.
  if recipe_type == RecipeType::OnCreate && root.is_none() && hex.is_none() {
    return Err(format!(
      "{}: on_create recipe {:?} must specify either 'root' or 'hex' to identify the target card",
      filename, id
    ));
  }

  let set_start = parse_set_start(recipe_value, filename, &id, "")?;

  let def = RecipeDef {
    index: stable_id,
    id: id.clone(),
    recipe_type,
    root,
    hex,
    slots,
    reagents,
    output_success,
    output_failure,
    duration,
    magnetic,
    interval,
    set_start,
  };
  Ok((id, stable_id, def))
}

/// Parse a `magnetic` field into a [`MagneticBucket`]. Same dispatch
/// logic as the top-level recipe file: bucket type ("stack" or
/// "on_create") plus direction-keyed arrays. Inner recipes are flattened
/// into `MagneticBucket.inners` in directional order.
///
/// The order matters — sub-id (the index a queued inner action carries
/// in its `flags`) is the inner's position in this flat list. Stable
/// across deploys as long as the JSON's direction keys and inner array
/// order don't change.
///
/// At most 16 inners per bucket (sub-id is 4 bits in `Action.flags`).
fn parse_magnetic_bucket(
  bucket: &serde_json::Map<String, Value>,
  filename: &str,
  parent_id: &str,
  type_ids: &BTreeMap<String, u8>,
) -> Result<MagneticBucket, String> {
  let bucket_type = bucket
    .get("type")
    .and_then(Value::as_str)
    .ok_or_else(|| format!("{}: recipe {:?} magnetic bucket missing 'type'", filename, parent_id))?;

  let direction_keys: &[(&str, RecipeType)] = match bucket_type {
    "stack" => &[
      ("up", RecipeType::Stack(StackDirection::Up)),
      ("down", RecipeType::Stack(StackDirection::Down)),
    ],
    "on_create" => &[("self", RecipeType::OnCreate)],
    other => {
      return Err(format!(
        "{}: recipe {:?} magnetic bucket has unknown type {:?}, expected \"stack\" or \"on_create\"",
        filename, parent_id, other,
      ));
    }
  };

  let mut inners: Vec<InnerRecipe> = Vec::new();
  for &(direction_key, recipe_type) in direction_keys {
    let Some(arr) = bucket.get(direction_key).and_then(Value::as_array) else {
      continue;
    };
    for (idx, inner_value) in arr.iter().enumerate() {
      let path = format!("magnetic.{}[{}]", direction_key, idx);
      inners.push(parse_inner_recipe(inner_value, recipe_type, filename, parent_id, &path, type_ids)?);
    }
  }

  if inners.len() > MAGNETIC_MAX_INNERS {
    return Err(format!(
      "{}: recipe {:?} magnetic bucket has {} inners (max {}, sub-id is 3 bits in Action.flags)",
      filename, parent_id, inners.len(), MAGNETIC_MAX_INNERS,
    ));
  }

  Ok(MagneticBucket { inners })
}

/// Hard cap on inner recipes per magnetic bucket. The queued inner
/// action stores its sub-id in 3 bits (bits 4..6) of `Action.flags`
/// — bit 7 is reserved for [`crate::actions::FLAG_ACTION_DEAD`] —
/// so 8 is the structural ceiling.
pub const MAGNETIC_MAX_INNERS: usize = 8;

/// Parse one inner recipe inside a magnetic bucket. Like
/// [`parse_recipe`] but: no `id`, no nested `magnetic`, no `interval`,
/// `recipe_type` is supplied by the caller from the bucket's direction
/// key. `duration` is required (it's the queued inner action's
/// duration). `path` is a JSON path fragment for error messages
/// (`"magnetic.up[0]"` etc.).
fn parse_inner_recipe(
  recipe_value: &Value,
  recipe_type: RecipeType,
  filename: &str,
  parent_id: &str,
  path: &str,
  type_ids: &BTreeMap<String, u8>,
) -> Result<InnerRecipe, String> {
  // Reject fields that don't apply to inner recipes — fail loud rather
  // than silently dropping authorial intent. `products` was the legacy
  // name for `output_success`; flagging it explicitly catches stale
  // recipe JSON.
  for forbidden in ["id", "magnetic", "interval", "products"] {
    if recipe_value.get(forbidden).is_some() {
      return Err(format!(
        "{}: recipe {:?} {}: inner recipe must not have '{}' field",
        filename, parent_id, path, forbidden
      ));
    }
  }

  let label = format!("{}/{}", parent_id, path);

  let root = if recipe_value.get("root").is_some() {
    Some(parse_entity(&recipe_value["root"], type_ids, filename, &label, "root")?)
  } else {
    None
  };

  let hex = if recipe_value.get("hex").is_some() {
    Some(parse_entity(&recipe_value["hex"], type_ids, filename, &label, "hex")?)
  } else {
    None
  };

  let slots = if let Some(slots_arr) = recipe_value.get("slots").and_then(Value::as_array) {
    slots_arr
      .iter()
      .enumerate()
      .map(|(i, v)| parse_entity(v, type_ids, filename, &label, &format!("slots[{}]", i)))
      .collect::<Result<Vec<_>, _>>()?
  } else {
    Vec::new()
  };

  let reagents = if let Some(arr) = recipe_value.get("reagents").and_then(Value::as_array) {
    arr
      .iter()
      .map(|v| parse_reagent(v, filename, &label))
      .collect::<Result<Vec<_>, _>>()?
  } else {
    Vec::new()
  };

  let output_success = parse_output_groups(
    recipe_value,
    "output_success",
    type_ids,
    filename,
    &label,
    path,
  )?;
  if recipe_value.get("output_fail").is_some() {
    return Err(format!(
      "{}: recipe {:?} {}: inner recipe must not have 'output_fail' field — \
       only the magnetic outer fires on failure",
      filename, parent_id, path
    ));
  }

  // Inner duration is *required* — it becomes the queued inner action's
  // duration in `actions`. Without it the queued action would have no
  // end time.
  let duration_value = recipe_value.get("duration").ok_or_else(|| {
    format!(
      "{}: recipe {:?} {}: inner recipe missing required 'duration' field",
      filename, parent_id, path
    )
  })?;
  let duration = parse_duration(duration_value, type_ids, filename, &label)?;

  let set_start = parse_set_start(recipe_value, filename, &label, path)?;

  Ok(InnerRecipe {
    recipe_type,
    root,
    hex,
    slots,
    reagents,
    output_success,
    duration,
    set_start,
  })
}

/// Parse the `"set_start"` key on a recipe value into a
/// [`SetStartFlags`] bitmask struct. Missing key → all-zero default
/// (set nothing). Each sub-key (`"root"`, `"slot"`, `"hex"`) holds an
/// array of card-flag names; each name maps to a fixed bit position in
/// `Card.flags` (see `content/cards/flags.json`). `"dead"` is rejected — it
/// cannot be set via `set_start`.
fn parse_set_start(
  recipe_value: &Value,
  filename: &str,
  id_label: &str,
  path_prefix: &str,
) -> Result<SetStartFlags, String> {
  let Some(obj) = recipe_value.get("set_start").and_then(Value::as_object) else {
    return Ok(SetStartFlags::default());
  };
  let where_str: String = if path_prefix.is_empty() {
    String::new()
  } else {
    format!(" {}", path_prefix)
  };
  let mut flags = SetStartFlags::default();
  for (sub_key, val) in obj {
    let bits_arr = val.as_array().ok_or_else(|| {
      format!(
        "{}: recipe {:?}{} set_start.{} not an array of flag names",
        filename, id_label, where_str, sub_key
      )
    })?;
    let mut mask: u8 = 0;
    for (i, bit_val) in bits_arr.iter().enumerate() {
      let name = bit_val.as_str().ok_or_else(|| {
        format!(
          "{}: recipe {:?}{} set_start.{}[{}] not a string",
          filename, id_label, where_str, sub_key, i
        )
      })?;
      let bit: u8 = match name {
        "position_hold"   => 0,
        "position_locked" => 1,
        "layer_locked"    => 2,
        "drop_hold"       => 3,
        "drop_locked"     => 4,
        "dead" => {
          return Err(format!(
            "{}: recipe {:?}{} set_start.{}[{}]: 'dead' cannot be set via set_start",
            filename, id_label, where_str, sub_key, i
          ));
        }
        other => {
          return Err(format!(
            "{}: recipe {:?}{} set_start.{}[{}]: unknown flag name {:?} \
             (expected \"position_hold\", \"position_locked\", \"layer_locked\", \
             \"drop_hold\", \"drop_locked\")",
            filename, id_label, where_str, sub_key, i, other
          ));
        }
      };
      mask |= 1u8 << bit;
    }
    match sub_key.as_str() {
      "root" => flags.root = mask,
      "slot" => flags.slot = mask,
      "hex"  => flags.hex  = mask,
      other => {
        return Err(format!(
          "{}: recipe {:?}{} set_start unknown sub-key {:?} \
           (expected \"root\", \"slot\", or \"hex\")",
          filename, id_label, where_str, other
        ));
      }
    }
  }
  Ok(flags)
}

/// Parse the nested `{ place: { owner: [entities…] } }` map under
/// either `output_success` or `output_fail` into a flat
/// [`ProductGroup`] list. Missing key → empty vec (no outputs is a
/// valid recipe). `path_prefix` is `""` for outer recipes; for inner
/// recipes it's the bucket path (e.g. `"magnetic.up[0]"`) so error
/// messages still point at the offending JSON location.
fn parse_output_groups(
  recipe_value: &Value,
  key: &str,
  type_ids: &BTreeMap<String, u8>,
  filename: &str,
  id_label: &str,
  path_prefix: &str,
) -> Result<Vec<ProductGroup>, String> {
  let Some(obj) = recipe_value.get(key).and_then(Value::as_object) else {
    return Ok(Vec::new());
  };
  let where_str: String = if path_prefix.is_empty() {
    String::new()
  } else {
    format!(" {}", path_prefix)
  };
  let mut groups: Vec<ProductGroup> = Vec::new();
  for (place_name, place_value) in obj {
    let place = match place_name.as_str() {
      "inventory" => ProductPlace::Inventory,
      other => {
        return Err(format!(
          "{}: recipe {:?}{} unknown product place {:?} under {}, expected \"inventory\"",
          filename, id_label, where_str, other, key
        ));
      }
    };
    let place_obj = place_value.as_object().ok_or_else(|| {
      format!(
        "{}: recipe {:?}{} {}[{}] not an object (expected `{{ owner: [entities…] }}`)",
        filename, id_label, where_str, key, place_name
      )
    })?;
    for (owner_name, entities_value) in place_obj {
      let owner = match owner_name.as_str() {
        "root" => ProductOwner::Root,
        "actor" => ProductOwner::Actor,
        "hex" => ProductOwner::Hex,
        "action" => ProductOwner::Action,
        other => {
          return Err(format!(
            "{}: recipe {:?}{} unknown product owner {:?} under {}[{}], expected \"root\", \"actor\", \"hex\", \"action\"",
            filename, id_label, where_str, other, key, place_name
          ));
        }
      };
      let entities_arr = entities_value.as_array().ok_or_else(|| {
        format!(
          "{}: recipe {:?}{} {}[{}][{}] not an array",
          filename, id_label, where_str, key, place_name, owner_name
        )
      })?;
      let entities = entities_arr
        .iter()
        .enumerate()
        .map(|(i, v)| {
          parse_entity(
            v,
            type_ids,
            filename,
            id_label,
            &format!("{}[{}][{}][{}]", key, place_name, owner_name, i),
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

/// Sentinel string parsed as `Entity::Any`. Reserved — a card with this
/// key would shadow the wildcard.
const ENTITY_ANY_LITERAL: &str = "any";
/// Prefix marking a string as `Entity::Type(<typename>)`. The remainder
/// of the string after `@` is looked up in the card-type registry at
/// recipe-build time.
const ENTITY_TYPE_PREFIX: char = '@';

/// Parse one entry from a recipe's `reagents` array. Strings `"root"`
/// and `"hex"` map to the named referents; integers `1..=255` map to
/// `Reagent::Slot`. Integer `0` is rejected with a hint to use
/// `"root"` instead — the old numeric-only encoding overloaded `0`
/// for "the chain root", and we want load-time errors when a recipe
/// file is on the old format.
fn parse_reagent(value: &Value, filename: &str, recipe_id: &str) -> Result<Reagent, String> {
  if let Some(s) = value.as_str() {
    return match s {
      "root" => Ok(Reagent::Root),
      "hex" => Ok(Reagent::Hex),
      other => Err(format!(
        "{}: recipe {:?} reagent string {:?} unknown — expected \"root\" or \"hex\"",
        filename, recipe_id, other
      )),
    };
  }
  if let Some(n) = value.as_u64() {
    if n == 0 {
      return Err(format!(
        "{}: recipe {:?} reagent index 0 not allowed — use \"root\" to consume the chain root",
        filename, recipe_id
      ));
    }
    if n > u8::MAX as u64 {
      return Err(format!(
        "{}: recipe {:?} reagent slot index {} exceeds u8 max",
        filename, recipe_id, n
      ));
    }
    return Ok(Reagent::Slot(n as u8));
  }
  Err(format!(
    "{}: recipe {:?} reagent {:?} not a string or non-negative integer",
    filename, recipe_id, value
  ))
}

fn parse_entity(
  value: &Value,
  type_ids: &BTreeMap<String, u8>,
  filename: &str,
  recipe_id: &str,
  path: &str,
) -> Result<Entity, String> {
  if let Some(s) = value.as_str() {
    if s == ENTITY_ANY_LITERAL {
      return Ok(Entity::Any);
    }
    if let Some(type_name) = s.strip_prefix(ENTITY_TYPE_PREFIX) {
      let &type_id = type_ids.get(type_name).ok_or_else(|| {
        format!(
          "{}: recipe {:?} {}: unknown card type {:?} (not declared in cards/types.json)",
          filename, recipe_id, path, type_name
        )
      })?;
      return Ok(Entity::Type(type_id));
    }
    return Ok(Entity::Card(s.to_string()));
  }
  let arr = value.as_array().ok_or_else(|| {
    format!(
      "{}: recipe {:?} {}: entity not a string or array: {:?}",
      filename, recipe_id, path, value
    )
  })?;
  match arr.len() {
    1 => parse_entity(&arr[0], type_ids, filename, recipe_id, path),
    2 => {
      // Disambiguate `[string, number]` (aspect check) from
      // `[entity, entity]` (AND). Numbers aren't valid entities, so a
      // numeric second element pins it to the aspect form.
      if let (Some(s), Some(n)) = (arr[0].as_str(), arr[1].as_i64()) {
        let id = aspect_id(s)?.ok_or_else(|| {
          format!(
            "{}: recipe {:?} {}: unknown aspect {:?} (not declared in aspects.json)",
            filename, recipe_id, path, s
          )
        })?;
        Ok(Entity::Aspect(id, n as i32))
      } else {
        let a = parse_entity(&arr[0], type_ids, filename, recipe_id, &format!("{}[0]", path))?;
        let b = parse_entity(&arr[1], type_ids, filename, recipe_id, &format!("{}[1]", path))?;
        Ok(Entity::And(Box::new(a), Box::new(b)))
      }
    }
    3 => {
      let middle = &arr[1];
      let a = parse_entity(&arr[0], type_ids, filename, recipe_id, &format!("{}[0]", path))?;
      let b = parse_entity(&arr[2], type_ids, filename, recipe_id, &format!("{}[2]", path))?;
      let middle_arr = middle.as_array().ok_or_else(|| {
        format!(
          "{}: recipe {:?} {}: 3-tuple middle not an array: {:?}",
          filename, recipe_id, path, middle
        )
      })?;
      if middle_arr.is_empty() {
        Ok(Entity::Or(Box::new(a), Box::new(b)))
      } else if middle_arr.len() == 2 {
        let weight_a = middle_arr[0].as_u64().ok_or_else(|| {
          format!(
            "{}: recipe {:?} {}: weight[0] not a non-negative integer: {:?}",
            filename, recipe_id, path, middle_arr[0]
          )
        })? as u32;
        let weight_b = middle_arr[1].as_u64().ok_or_else(|| {
          format!(
            "{}: recipe {:?} {}: weight[1] not a non-negative integer: {:?}",
            filename, recipe_id, path, middle_arr[1]
          )
        })? as u32;
        Ok(Entity::WeightedOr {
          a: Box::new(a),
          b: Box::new(b),
          weight_a,
          weight_b,
        })
      } else {
        Err(format!(
          "{}: recipe {:?} {}: 3-tuple middle has {} elements, expected 0 (Or) or 2 (WeightedOr)",
          filename,
          recipe_id,
          path,
          middle_arr.len()
        ))
      }
    }
    _ => Err(format!(
      "{}: recipe {:?} {}: entity array of length {} not supported",
      filename,
      recipe_id,
      path,
      arr.len()
    )),
  }
}

fn parse_duration(
  value: &Value,
  type_ids: &BTreeMap<String, u8>,
  filename: &str,
  recipe_id: &str,
) -> Result<Duration, String> {
  // Fixed: bare number.
  if let Some(n) = value.as_u64() {
    return Ok(Duration::Fixed(n as u32));
  }

  // Conditional: array of `[seconds, condition]` cases plus a trailing
  // bare-number fallback.
  let arr = value.as_array().ok_or_else(|| {
    format!(
      "{}: recipe {:?} duration not a number or array: {:?}",
      filename, recipe_id, value
    )
  })?;

  if arr.is_empty() {
    return Err(format!(
      "{}: recipe {:?} duration is an empty array",
      filename, recipe_id
    ));
  }

  let mut cases: Vec<(u32, Entity)> = Vec::new();
  let mut fallback: Option<u32> = None;

  for (i, entry) in arr.iter().enumerate() {
    if let Some(n) = entry.as_u64() {
      if i != arr.len() - 1 {
        return Err(format!(
          "{}: recipe {:?} duration[{}] is a bare number; only the trailing entry can be the fallback",
          filename, recipe_id, i
        ));
      }
      fallback = Some(n as u32);
      continue;
    }

    let case = entry.as_array().ok_or_else(|| {
      format!(
        "{}: recipe {:?} duration[{}] not a number or [seconds, condition]: {:?}",
        filename, recipe_id, i, entry
      )
    })?;
    if case.len() != 2 {
      return Err(format!(
        "{}: recipe {:?} duration[{}] not a 2-element [seconds, condition]",
        filename, recipe_id, i
      ));
    }
    let secs = case[0].as_u64().ok_or_else(|| {
      format!(
        "{}: recipe {:?} duration[{}][0] not a non-negative integer: {:?}",
        filename, recipe_id, i, case[0]
      )
    })? as u32;
    let cond = parse_entity(&case[1], type_ids, filename, recipe_id, &format!("duration[{}][1]", i))?;
    cases.push((secs, cond));
  }

  let fallback = fallback.ok_or_else(|| {
    format!(
      "{}: recipe {:?} duration: no trailing fallback (last entry must be a bare number)",
      filename, recipe_id
    )
  })?;

  Ok(Duration::Conditional { cases, fallback })
}

