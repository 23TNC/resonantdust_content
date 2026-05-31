//! Storage-agnostic recipe **plan** pieces — the pure parts of the legacy
//! `shard::action_completion` that compute *what* an action does without
//! touching storage.
//!
//! Today this is `compute_holds` (which cards a validated action claims, and
//! how) plus the `Effect`/`HoldKinds` vocabulary the gateway's apply step
//! decomposes into cross-DB reducer calls. The output-tape executor
//! (`execute_stmt` → the `Effect` list) is ported here next, over the
//! `CardStore`/`ZoneStore` traits from [`crate::recipe_validate`]; for now the
//! gateway computes holds (pure) and applies them + the dedup gate.

use std::collections::BTreeMap;

use crate::definition_core::{
    aspect_id as core_aspect_id, decode_definition, is_aspect_descendant,
};
use crate::packed::{pack_macro_zone_full, INVENTORY_LAYER};
use crate::recipe_core::{Iterator as RecipeIterator, Recipe, Seg, Stmt};
use crate::recipe_statement::{parse_statement, Segment, StatementValue};
use crate::recipe_validate::{micro_is_card, CardStore, SyntheticTile};

/// Per-card hold kinds a recipe claims at apply time. Mirrors the legacy
/// `shard::action_completion::HoldKinds`; the gateway maps each set bit to an
/// `acquire_hold(kind)` reducer call and the inverse at completion.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldKinds {
    /// Exclusive claim — `FLAG_SLOT_HOLD` (`acquire_slot_hold`).
    pub slot_hold: bool,
    /// Position pin — `acquire_position_hold`.
    pub position_hold: bool,
    /// Shared borrow — `acquire_slot_share`.
    pub slot_share: bool,
}

/// Build the `(card_id → HoldKinds)` map for a `(recipe, bindings, root)`
/// triple. Direct port of `shard::action_completion::compute_holds` — pure, no
/// storage: the root anchor's tokens unioned with each iterator's
/// slot_hold/share + position_hold flavors over its bound cards. Multiple paths
/// to the same card union by field-OR.
pub fn compute_holds(
    recipe: &Recipe,
    bindings: &[Vec<u32>],
    root: u32,
) -> BTreeMap<u32, HoldKinds> {
    let mut holds: BTreeMap<u32, HoldKinds> = BTreeMap::new();

    // Root anchor (independent of iterator promotion).
    if recipe.anchors.root && root != 0 {
        let entry = holds.entry(root).or_default();
        if recipe.root_slot_hold {
            entry.slot_hold = true;
        } else {
            entry.slot_share = true;
        }
        if recipe.root_position_hold {
            entry.position_hold = true;
        }
    }

    // Iterator bindings (binding row `i` ↔ iterator `i`).
    for (i, it) in recipe.iterators.iter().enumerate() {
        let Some(row) = bindings.get(i) else { continue };
        for &card_id in row {
            if card_id == 0 {
                continue;
            }
            let entry = holds.entry(card_id).or_default();
            if it.slot_hold {
                entry.slot_hold = true;
            } else {
                entry.slot_share = true;
            }
            if it.position_hold {
                entry.position_hold = true;
            }
        }
    }

    holds
}

/// HoldKinds the recipe assigns to its **synthetic-tile** slot(s) — the iterators
/// whose binding contains the sentinel `0` (an un-promoted tile). Returns `None`
/// when **no** slot binds the tile: the recipe doesn't reference it, so the
/// gateway must NOT promote one (else any recipe fired on a cell that happens to
/// carry a tile — e.g. `corpus-` sitting on a tile — would spawn a spurious
/// tile-card). `compute_holds` skips the sentinel (it has no `card_id`), so when
/// the tile IS bound the gateway resolves its hold here and acquires it on the
/// promoted tile-card by position — same `(slot_hold, position_hold)` mapping as
/// a real binding (`use`/`claim` → `slot_hold`, `share`/`borrow` → `slot_share`);
/// ORs across slots.
pub fn synthetic_tile_holds(recipe: &Recipe, bindings: &[Vec<u32>]) -> Option<HoldKinds> {
    let mut kinds = HoldKinds::default();
    let mut bound = false;
    for (i, it) in recipe.iterators.iter().enumerate() {
        let Some(row) = bindings.get(i) else { continue };
        if row.iter().any(|&id| id == 0) {
            bound = true;
            if it.slot_hold {
                kinds.slot_hold = true;
            } else {
                kinds.slot_share = true;
            }
            if it.position_hold {
                kinds.position_hold = true;
            }
        }
    }
    bound.then_some(kinds)
}

// ===== output tape → effects (storage-agnostic plan) =====================
//
// Port of the card-only half of `shard::action_completion`'s `plan()` /
// `execute_*` tape walker. Reads go through [`CardStore`] (the gateway's
// gathered snapshot); the walk emits an [`Effect`] list the gateway's apply
// step decomposes into reducer calls. World-terrain output ops (tile-stock,
// blueprint unlock, player faction) are rejected here for now — they return a
// clear error rather than an effect, deferred to the cross-DB follow-up.

const VAR_SLOT_COUNT: usize = 8;
const PROGRESS_STYLE_NONE: u8 = 0;
const PROGRESS_STYLE_LTR: u8 = 1;
const PROGRESS_STYLE_RTL: u8 = 2;

/// A completion-time effect accumulated by walking a recipe's output tape.
/// Card-only kinds today; the gateway maps each to a `cards` reducer call
/// future-stamped at `completion_ms`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Mark a card dead (`destroy_card`).
    Destroy { card_id: u32 },
    /// Spawn a card into an owner's inventory bucket (`create_card`).
    Create {
        def_key: String,
        surface: u8,
        macro_zone: u64,
        owner_id: u32,
    },
    /// Spawn a deferred stack member anchored to `host_card_id`; the client
    /// cascade resolves the concrete cell at mirror time (`create_card` with a
    /// deferred placement).
    CreateDeferred { def_key: String, host_card_id: u32 },
    /// Mutate the synthetic tile's per-cell stock `slot` (`aspect.X.{sub,add,set}`).
    /// The gateway fills in surface/macro_zone/cell from the proposal and routes
    /// this to the `regions` `modify_tile_stock` reducer (promote-then-mutate).
    ModifyTileStock { slot: u8, op: StockOp, delta: u8 },
    /// Set a blueprint's discovery bit on the target soul's `SoulPrivate`
    /// (`<soul>.blueprint.unlock: <key>`). Routed to the cards `unlock_blueprint`
    /// reducer. Idempotent.
    UnlockBlueprint { blueprint_key: String, target_card_id: u32 },
}

/// Tile-stock arithmetic for [`Effect::ModifyTileStock`]. `code()` is the u8 the
/// gateway passes to the regions `modify_tile_stock` reducer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockOp {
    Sub,
    Add,
    Set,
}

impl StockOp {
    pub fn code(self) -> u8 {
        match self {
            StockOp::Sub => 0,
            StockOp::Add => 1,
            StockOp::Set => 2,
        }
    }
}

/// Pre-computed output of a recipe's tape walk. `duration_ms` feeds the
/// `completion_ms` future-stamp; `holds` is shared with the acquire/release
/// passes; `effects` are emitted at completion. Mirrors
/// `shard::action_completion::ActionPlan` minus the storage handle.
#[derive(Clone, Debug)]
pub struct ActionPlan {
    /// `card_id → progress_style` for completion-row progress bars. The gateway
    /// may ignore these in v1 (cosmetic).
    pub styles: BTreeMap<u32, u8>,
    /// Action duration in seconds (`sys.duration.set`).
    pub duration: u32,
    /// Completion-time effects, in tape order.
    pub effects: Vec<Effect>,
    /// Per-card holds the action claims (`compute_holds`).
    pub holds: BTreeMap<u32, HoldKinds>,
    /// Holds for the action's **synthetic tile** (the sentinel-`0` slot), when the
    /// recipe targets one. `compute_holds` skips the sentinel (no card_id), so the
    /// gateway promotes the tile up front and acquires *these* on it by position
    /// — exactly the kinds the tile slot's verb declares. `None` when the recipe
    /// has no synthetic tile.
    pub tile_holds: Option<HoldKinds>,
}

impl ActionPlan {
    /// Milliseconds from start to completion (`duration * 1000`).
    pub fn duration_ms(&self) -> u64 {
        (self.duration as u64) * 1000
    }
}

/// Tape-walk accumulator (private). Mirrors `shard`'s `TapeWalker`.
struct TapeWalker {
    vars: [i32; VAR_SLOT_COUNT],
    duration: u32,
    styles: BTreeMap<u32, u8>,
    pending: Vec<Effect>,
    /// The synthetic tile substituted for the branch-0 sentinel, if the action
    /// targets one (the gateway derives it from the gathered zone). Read by the
    /// tile-stock output op.
    synthetic: Option<SyntheticTile>,
}

impl TapeWalker {
    fn new(synthetic: Option<SyntheticTile>) -> Self {
        Self {
            vars: [0; VAR_SLOT_COUNT],
            duration: 0,
            styles: BTreeMap::new(),
            pending: Vec::new(),
            synthetic,
        }
    }
}

/// Walk `recipe.output` into an [`ActionPlan`] over a read-only [`CardStore`].
/// `now_ms` scopes card reads (the gateway's snapshot ignores it — it's
/// latest-version only). Errors name the failing output statement.
pub fn plan_output<S: CardStore>(
    store: &S,
    recipe: &Recipe,
    bindings: &[Vec<u32>],
    root: u32,
    synthetic: Option<SyntheticTile>,
    now_ms: u64,
) -> Result<ActionPlan, String> {
    // Hold the synthetic tile only when the recipe actually BINDS it (a sentinel-0
    // slot) AND the gate derived one. A recipe with no tile slot (e.g. `corpus-`
    // fired on a card that happens to sit on a tile) must not promote a tile.
    let tile_holds = synthetic
        .is_some()
        .then(|| synthetic_tile_holds(recipe, bindings))
        .flatten();
    let mut walker = TapeWalker::new(synthetic);
    for (i, stmt) in recipe.output.iter().enumerate() {
        execute_stmt(store, &mut walker, recipe, stmt, bindings, root, now_ms)
            .map_err(|e| format!("output[{i}]: {e}"))?;
    }
    let holds = compute_holds(recipe, bindings, root);
    Ok(ActionPlan {
        styles: walker.styles,
        duration: walker.duration,
        effects: walker.pending,
        holds,
        tile_holds,
    })
}

fn execute_stmt<S: CardStore>(
    store: &S,
    walker: &mut TapeWalker,
    recipe: &Recipe,
    stmt: &Stmt,
    bindings: &[Vec<u32>],
    root: u32,
    now_ms: u64,
) -> Result<(), String> {
    let segs = stmt.segments.as_slice();
    let head = match segs.first() {
        Some(Seg::Word(w)) => w.as_str(),
        Some(Seg::Slot { .. }) => {
            return execute_card_op(store, walker, recipe, stmt, bindings, root, now_ms);
        }
        Some(Seg::Index(_)) | None => {
            return Err(format!("malformed statement segments: {segs:?}"));
        }
    };
    match head {
        "when" => execute_when(store, walker, recipe, stmt, bindings, root, now_ms),
        "sys" => execute_sys(walker, stmt),
        "var" => execute_var(store, walker, recipe, stmt, bindings, root, now_ms),
        "root" => execute_card_op(store, walker, recipe, stmt, bindings, root, now_ms),
        other => Err(format!("unsupported statement head {other:?}")),
    }
}

// ----- sys.<slot>.set ---------------------------------------------------

fn execute_sys(walker: &mut TapeWalker, stmt: &Stmt) -> Result<(), String> {
    let segs = stmt.segments.as_slice();
    if segs.len() != 3 {
        return Err(format!("sys: expected `sys.<slot>.set: <value>`, got {segs:?}"));
    }
    let slot = match &segs[1] {
        Seg::Word(w) => w.as_str(),
        _ => return Err("sys: second segment must be a slot name".to_string()),
    };
    match &segs[2] {
        Seg::Word(w) if w == "set" => {}
        other => return Err(format!("sys: third segment must be `set`, got {other:?}")),
    }
    match slot {
        "duration" => {
            let n = match &stmt.value {
                Some(StatementValue::Int(n)) => *n as u32,
                _ => return Err("sys.duration.set: requires integer value".to_string()),
            };
            walker.duration = n;
        }
        other => return Err(format!("sys: unknown slot {other:?}")),
    }
    Ok(())
}

/// Decode a `style.set` value — named strings (`ltr`/`rtl`/`none`) or raw 0..=7.
fn style_from_value(value: &Option<StatementValue>) -> Result<u8, String> {
    match value {
        Some(StatementValue::Str(s)) => match s.as_str() {
            "none" => Ok(PROGRESS_STYLE_NONE),
            "ltr" => Ok(PROGRESS_STYLE_LTR),
            "rtl" => Ok(PROGRESS_STYLE_RTL),
            other => Err(format!("style.set: unknown style {other:?}")),
        },
        Some(StatementValue::Int(n)) => Ok((*n as u8) & 0b111),
        None => Err("style.set: requires a value".to_string()),
    }
}

// ----- var.N.set / add / sub --------------------------------------------

fn execute_var<S: CardStore>(
    store: &S,
    walker: &mut TapeWalker,
    recipe: &Recipe,
    stmt: &Stmt,
    bindings: &[Vec<u32>],
    root: u32,
    now_ms: u64,
) -> Result<(), String> {
    let segs = stmt.segments.as_slice();
    if segs.len() != 3 {
        return Err(format!("var: expected `var.N.<op>: <value>`, got {segs:?}"));
    }
    let var_idx = match &segs[1] {
        Seg::Index(n) => *n as usize,
        _ => return Err("var: second segment must be a variable index".to_string()),
    };
    if var_idx >= VAR_SLOT_COUNT {
        return Err(format!(
            "var.{var_idx}: index out of range (max {})",
            VAR_SLOT_COUNT - 1
        ));
    }
    let op = match &segs[2] {
        Seg::Word(w) => w.as_str(),
        _ => return Err("var: third segment must be an op word".to_string()),
    };
    let operand = match &stmt.value {
        Some(StatementValue::Int(n)) => *n as i32,
        Some(StatementValue::Str(path_str)) => {
            read_path_value(store, recipe, path_str, bindings, root, now_ms)
                .map_err(|e| format!("var.{var_idx}.{op}: path-RHS read {path_str:?}: {e}"))?
        }
        None => return Err(format!("var.{var_idx}.{op}: requires a value")),
    };
    match op {
        "set" => walker.vars[var_idx] = operand,
        "add" => walker.vars[var_idx] = walker.vars[var_idx].saturating_add(operand),
        "sub" => walker.vars[var_idx] = walker.vars[var_idx].saturating_sub(operand),
        other => return Err(format!("var.{var_idx}: unsupported op {other:?}")),
    }
    Ok(())
}

// ----- when.<predicate>.<inner_statement> -------------------------------

fn execute_when<S: CardStore>(
    store: &S,
    walker: &mut TapeWalker,
    recipe: &Recipe,
    stmt: &Stmt,
    bindings: &[Vec<u32>],
    root: u32,
    now_ms: u64,
) -> Result<(), String> {
    let segs = stmt.segments.as_slice();
    let cmp_idx = (1..segs.len())
        .find(|&i| match &segs[i] {
            Seg::Word(w) => matches!(w.as_str(), "gt" | "ge" | "lt" | "le" | "eq" | "ne"),
            _ => false,
        })
        .ok_or_else(|| format!("when: no comparison op (gt/ge/lt/le/eq/ne) found in {segs:?}"))?;
    if cmp_idx + 1 >= segs.len() {
        return Err("when: comparison op needs a value segment after it".to_string());
    }
    let pred_path = &segs[1..cmp_idx];
    let cmp_op = match &segs[cmp_idx] {
        Seg::Word(w) => w.as_str(),
        _ => unreachable!(),
    };
    let cmp_value = match &segs[cmp_idx + 1] {
        Seg::Index(n) => *n as i32,
        Seg::Word(_) => return Err("when: comparison value must be an integer".to_string()),
        Seg::Slot { .. } => return Err("when: comparison value cannot be a slot ref".to_string()),
    };
    let inner_segs = segs[cmp_idx + 2..].to_vec();
    if inner_segs.is_empty() {
        return Err("when: missing inner statement after predicate".to_string());
    }
    let pred_value = match pred_path {
        [Seg::Word(w), Seg::Index(n)] if w == "var" => *walker
            .vars
            .get(*n as usize)
            .ok_or_else(|| format!("when: var.{n} out of range"))?,
        other => {
            return Err(format!(
                "when: predicate path must be `var.N` today; got {other:?}"
            ))
        }
    };
    let matched = match cmp_op {
        "gt" => pred_value > cmp_value,
        "ge" => pred_value >= cmp_value,
        "lt" => pred_value < cmp_value,
        "le" => pred_value <= cmp_value,
        "eq" => pred_value == cmp_value,
        "ne" => pred_value != cmp_value,
        _ => unreachable!(),
    };
    if !matched {
        return Ok(());
    }
    let inner_stmt = Stmt {
        segments: inner_segs,
        value: stmt.value.clone(),
        slot_hold: stmt.slot_hold,
        position_hold: stmt.position_hold,
    };
    execute_stmt(store, walker, recipe, &inner_stmt, bindings, root, now_ms)
}

// ----- card ops: destroy / create / style -------------------------------

fn execute_card_op<S: CardStore>(
    store: &S,
    walker: &mut TapeWalker,
    recipe: &Recipe,
    stmt: &Stmt,
    bindings: &[Vec<u32>],
    root: u32,
    now_ms: u64,
) -> Result<(), String> {
    let segs = stmt.segments.as_slice();
    let op = match segs.last() {
        Some(Seg::Word(w)) => w.as_str(),
        _ => return Err(format!("card op: last segment must be a word; got {segs:?}")),
    };
    let path = &segs[..segs.len() - 1];

    match op {
        "destroy" => {
            let card_id = resolve_card_target(store, recipe, path, bindings, root, now_ms)?;
            walker.pending.push(Effect::Destroy { card_id });
            Ok(())
        }
        "create" => {
            let def_key = match &stmt.value {
                Some(StatementValue::Str(s)) => s.clone(),
                _ => return Err("create: requires a string def_id value".to_string()),
            };
            let last = path.last();
            let penultimate = path.get(path.len().wrapping_sub(2));
            let is_inventory_suffix = matches!(last, Some(Seg::Word(w)) if w == "inventory");
            let is_stack_n_suffix = matches!(last, Some(Seg::Index(_)))
                && matches!(penultimate, Some(Seg::Word(w)) if w == "stack");

            if is_inventory_suffix {
                let target_path = &path[..path.len() - 1];
                let owner_card_id =
                    resolve_card_target(store, recipe, target_path, bindings, root, now_ms)?;
                walker.pending.push(Effect::Create {
                    def_key,
                    surface: INVENTORY_LAYER,
                    macro_zone: pack_macro_zone_full(owner_card_id, INVENTORY_LAYER, 0, 0),
                    owner_id: owner_card_id,
                });
                Ok(())
            } else if is_stack_n_suffix {
                let host_path = &path[..path.len() - 2];
                let host_card_id =
                    resolve_card_target(store, recipe, host_path, bindings, root, now_ms)?;
                walker
                    .pending
                    .push(Effect::CreateDeferred { def_key, host_card_id });
                Ok(())
            } else {
                Err(format!(
                    "create: path must end in `.inventory.create` or `.stack.<N>.create`; got {segs:?}"
                ))
            }
        }
        "set" if matches!(path.last(), Some(Seg::Word(w)) if w == "style") => {
            let target_path = &path[..path.len() - 1];
            let target_id = resolve_card_target(store, recipe, target_path, bindings, root, now_ms)?;
            let style = style_from_value(&stmt.value)?;
            walker.styles.insert(target_id, style);
            Ok(())
        }
        "unlock" => {
            // `<path>.blueprint.unlock: <key>` — set the blueprint discovery bit
            // on the resolved soul. The path before `.unlock` must end in
            // `.blueprint`; the target soul is everything before that.
            if path.last().and_then(|s| match s {
                Seg::Word(w) => Some(w.as_str()),
                _ => None,
            }) != Some("blueprint")
            {
                return Err(format!("unlock: path must end in `.blueprint.unlock`; got {segs:?}"));
            }
            let target_path = &path[..path.len() - 1];
            let target_card_id =
                resolve_card_target(store, recipe, target_path, bindings, root, now_ms)?;
            let blueprint_key = match &stmt.value {
                Some(StatementValue::Str(s)) => s.clone(),
                _ => return Err("unlock: requires a string blueprint key value".to_string()),
            };
            walker.pending.push(Effect::UnlockBlueprint {
                blueprint_key,
                target_card_id,
            });
            Ok(())
        }
        "sub" | "add" | "set" => {
            // `<target>.aspect.<name>.<op>: <delta>` — tile-stock mutation on the
            // synthetic tile. (Player `aspect.faction.set` is still deferred.)
            if path.len() < 2 {
                return Err(format!("stock op: short path {segs:?}"));
            }
            match &path[path.len() - 2] {
                Seg::Word(w) if w == "aspect" => {}
                other => {
                    return Err(format!("stock op: expected `.aspect.<name>.<op>`; got {other:?}"))
                }
            }
            let aspect_name = match &path[path.len() - 1] {
                Seg::Word(w) => w.as_str(),
                other => return Err(format!("stock op: aspect name must be a word; got {other:?}")),
            };
            if aspect_name == "faction" {
                return Err(
                    "gateway v1: `aspect.faction.set` (player faction) not yet supported".to_string(),
                );
            }
            let delta = match &stmt.value {
                Some(StatementValue::Int(n)) => (*n).clamp(0, 255) as u8,
                _ => return Err("stock op: requires an integer value".to_string()),
            };
            // Target must be the synthetic tile (branch-0 sentinel).
            let target_path = &path[..path.len() - 2];
            let (packed_def, _stocks) =
                resolve_synthetic_target(recipe, target_path, bindings, walker.synthetic)?;
            // Resolve the aspect → the tile def's stock slot (sub-aspect widening:
            // a stock slot declared for `wood` matches an `aspect.wood` op).
            let aspect = core_aspect_id(aspect_name)
                .map_err(|e| format!("aspect lookup {aspect_name:?}: {e}"))?
                .ok_or_else(|| format!("unknown aspect {aspect_name:?}"))?;
            let def = decode_definition(packed_def)
                .map_err(|e| format!("decode tile def: {e}"))?
                .ok_or_else(|| format!("tile packed {packed_def:#06x} has no def"))?;
            let slot = def
                .stock
                .iter()
                .position(|s| is_aspect_descendant(s.aspect_id, aspect).unwrap_or(false))
                .ok_or_else(|| {
                    format!(
                        "stock op: tile def {:?} declares no stock slot for aspect {aspect_name:?}",
                        def.key
                    )
                })?;
            let stock_op = match op {
                "sub" => StockOp::Sub,
                "add" => StockOp::Add,
                "set" => StockOp::Set,
                _ => unreachable!(),
            };
            walker.pending.push(Effect::ModifyTileStock {
                slot: slot as u8,
                op: stock_op,
                delta,
            });
            Ok(())
        }
        other => Err(format!("card op: unsupported op {other:?}")),
    }
}

/// Validate that `target_path` is the synthetic-tile slot (a single top-level
/// branch-0 slot bound to the `0` sentinel) and return the synthetic tile.
fn resolve_synthetic_target(
    recipe: &Recipe,
    target_path: &[Seg],
    bindings: &[Vec<u32>],
    synthetic: Option<SyntheticTile>,
) -> Result<SyntheticTile, String> {
    let synth = synthetic
        .ok_or_else(|| "tile stock op: no synthetic tile for this action".to_string())?;
    match target_path {
        [Seg::Slot { iterator_id, offset }] => {
            let it = recipe
                .iterators
                .get(*iterator_id as usize)
                .ok_or_else(|| format!("iterator {iterator_id} out of range"))?;
            let bound = bindings
                .get(*iterator_id as usize)
                .and_then(|row| row.get(*offset as usize))
                .copied()
                .unwrap_or(0);
            if it.parent.is_empty() && it.branch == 0 && *offset == 0 && bound == 0 {
                Ok(synth)
            } else {
                Err("tile stock op: target is not the synthetic tile (branch-0 sentinel)".to_string())
            }
        }
        other => Err(format!("tile stock op: target must be a single tile slot; got {other:?}")),
    }
}

// ----- path resolution (storage-agnostic) -------------------------------

/// Resolve a segment path to a terminal `card_id`. Port of
/// `shard::action_completion::resolve_card_target` over [`CardStore`].
fn resolve_card_target<S: CardStore>(
    store: &S,
    recipe: &Recipe,
    path: &[Seg],
    bindings: &[Vec<u32>],
    root: u32,
    now_ms: u64,
) -> Result<u32, String> {
    let _ = recipe;
    let mut card_id = match path.first() {
        Some(Seg::Word(w)) if w == "root" => {
            if root == 0 {
                return Err("resolve: root is 0".to_string());
            }
            root
        }
        Some(Seg::Slot { iterator_id, offset }) => {
            let binding_row = bindings
                .get(*iterator_id as usize)
                .ok_or_else(|| format!("bindings missing iterator {iterator_id}"))?;
            *binding_row.get(*offset as usize).ok_or_else(|| {
                format!(
                    "iterator {iterator_id} offset {offset} out of range (binding len {})",
                    binding_row.len()
                )
            })?
        }
        other => return Err(format!("resolve: unsupported anchor {other:?}")),
    };

    let mut i = 1;
    while i < path.len() {
        match &path[i] {
            Seg::Word(w) if w == "owner" => {
                let card = store
                    .card_at(card_id, now_ms)
                    .ok_or_else(|| format!("resolve: card {card_id} not found"))?;
                if card.owner_id == 0 {
                    return Err(format!("resolve: card {card_id} has no owner"));
                }
                card_id = card.owner_id;
                i += 1;
            }
            Seg::Word(w) if w == "parent" => {
                let card = store
                    .card_at(card_id, now_ms)
                    .ok_or_else(|| format!("resolve: card {card_id} not found"))?;
                if !micro_is_card(&card) {
                    return Err(format!(
                        "resolve: card {card_id} has no parent (it is a chain root)"
                    ));
                }
                card_id = card.micro_location;
                i += 1;
            }
            Seg::Slot { iterator_id, offset } => {
                let binding_row = bindings
                    .get(*iterator_id as usize)
                    .ok_or_else(|| format!("bindings missing iterator {iterator_id}"))?;
                card_id = *binding_row.get(*offset as usize).ok_or_else(|| {
                    format!("iterator {iterator_id} offset {offset} out of range")
                })?;
                i += 1;
            }
            other => return Err(format!("resolve: unsupported path segment {other:?}")),
        }
    }
    if card_id == 0 {
        return Err("resolve: terminal card_id is 0".to_string());
    }
    Ok(card_id)
}

/// Collapse raw `[Word("slot"), Index(B), Index(N)]` triplets into `Seg::Slot`
/// by matching the recipe's existing iterators (read-only). Port of
/// `shard::action_completion::collapse_slots_readonly`.
fn collapse_slots_readonly(
    raw: &[Segment],
    iterators: &[RecipeIterator],
) -> Result<Vec<Seg>, String> {
    let mut out: Vec<Seg> = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if i + 2 < raw.len() && raw[i].as_word() == Some("slot") {
            if let (Some(branch_u32), Some(offset)) = (raw[i + 1].as_index(), raw[i + 2].as_index())
            {
                if branch_u32 > 255 {
                    return Err(format!("slot.{branch_u32}.{offset}: branch must fit in u8"));
                }
                let branch = branch_u32 as u8;
                let parent_slice: &[Seg] = &out;
                let iter_id = iterators
                    .iter()
                    .position(|it| it.parent.as_slice() == parent_slice && it.branch == branch)
                    .ok_or_else(|| {
                        format!(
                            "slot.{branch}.{offset}: no iterator in recipe with parent={parent_slice:?} branch={branch}"
                        )
                    })?;
                out.push(Seg::Slot {
                    iterator_id: iter_id as u32,
                    offset,
                });
                i += 3;
                continue;
            }
        }
        out.push(match &raw[i] {
            Segment::Word(w) => Seg::Word(w.clone()),
            Segment::Index(n) => Seg::Index(*n),
        });
        i += 1;
    }
    Ok(out)
}

/// Read an aspect value at runtime from a `<card-path>.aspect.<name>` string.
/// Port of `shard::action_completion::read_path_value` over [`CardStore`].
fn read_path_value<S: CardStore>(
    store: &S,
    recipe: &Recipe,
    path_str: &str,
    bindings: &[Vec<u32>],
    root: u32,
    now_ms: u64,
) -> Result<i32, String> {
    let raw = parse_statement(path_str).map_err(|e| format!("parse path: {e}"))?;
    let segs = &raw.path;
    if segs.len() < 3 {
        return Err(format!(
            "read_path_value: path too short for `.aspect.<name>` terminal: {path_str:?}"
        ));
    }
    match &segs[segs.len() - 2] {
        Segment::Word(w) if w == "aspect" => {}
        other => {
            return Err(format!(
                "read_path_value: expected `.aspect.<name>` terminal; got {other:?}"
            ))
        }
    }
    let aspect_name = match &segs[segs.len() - 1] {
        Segment::Word(w) => w.as_str(),
        other => {
            return Err(format!(
                "read_path_value: aspect name must be a word; got {other:?}"
            ))
        }
    };
    let aspect = core_aspect_id(aspect_name)
        .map_err(|e| format!("aspect lookup {aspect_name:?}: {e}"))?
        .ok_or_else(|| format!("unknown aspect {aspect_name:?}"))?;

    let card_path = &segs[..segs.len() - 2];
    let resolved_path = collapse_slots_readonly(card_path, &recipe.iterators)?;
    let card_id = resolve_card_target(store, recipe, &resolved_path, bindings, root, now_ms)?;

    let card = store
        .card_at(card_id, now_ms)
        .ok_or_else(|| format!("read_path_value: card {card_id} not found"))?;
    let def = decode_definition(card.packed_definition)
        .map_err(|e| format!("decode def: {e}"))?
        .ok_or_else(|| format!("card {card_id} has unknown def"))?;
    let mut total: i32 = 0;
    for (id, v) in &def.aspects {
        if is_aspect_descendant(*id, aspect).unwrap_or(false) {
            total += *v as i32;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe_statement::StatementValue;
    use crate::recipe_tape::{AnchorSet, Iterator, Recipe};
    use crate::recipe_validate::CardView;
    use std::collections::HashMap;

    struct Mock(HashMap<u32, CardView>);
    impl CardStore for Mock {
        fn card_at(&self, id: u32, _t: u64) -> Option<CardView> {
            self.0.get(&id).cloned()
        }
    }
    fn word(w: &str) -> Seg {
        Seg::Word(w.to_string())
    }
    fn stmt(segs: Vec<Seg>, value: Option<StatementValue>) -> Stmt {
        Stmt {
            segments: segs,
            value,
            slot_hold: false,
            position_hold: false,
        }
    }

    #[test]
    fn plan_output_duration_create_destroy() {
        // output: set duration, spawn into root's inventory, destroy root.
        let recipe = Recipe {
            id: "t".to_string(),
            input: vec![],
            output: vec![
                stmt(
                    vec![word("sys"), word("duration"), word("set")],
                    Some(StatementValue::Int(5)),
                ),
                stmt(
                    vec![word("root"), word("inventory"), word("create")],
                    Some(StatementValue::Str("berries".to_string())),
                ),
                stmt(vec![word("root"), word("destroy")], None),
            ],
            iterators: vec![],
            anchors: AnchorSet {
                root: true,
                branches: 0,
            },
            root_slot_hold: true,
            root_position_hold: false,
        };
        // resolve_card_target on `[root]` returns root directly — no reads needed.
        let store = Mock(HashMap::new());
        let plan = plan_output(&store, &recipe, &[], 100, None, 0).expect("plan");
        assert_eq!(plan.duration, 5);
        assert_eq!(plan.duration_ms(), 5000);
        assert_eq!(plan.effects.len(), 2);
        assert!(matches!(
            &plan.effects[0],
            Effect::Create { owner_id: 100, surface, .. } if *surface == INVENTORY_LAYER
        ));
        assert_eq!(plan.effects[1], Effect::Destroy { card_id: 100 });
    }

    #[test]
    fn plan_output_blueprint_unlock() {
        let recipe = Recipe {
            id: "t".to_string(),
            input: vec![],
            output: vec![stmt(
                vec![word("root"), word("blueprint"), word("unlock")],
                Some(StatementValue::Str("nd_furnace".to_string())),
            )],
            iterators: vec![],
            anchors: AnchorSet {
                root: true,
                branches: 0,
            },
            root_slot_hold: true,
            root_position_hold: false,
        };
        let store = Mock(HashMap::new());
        let plan = plan_output(&store, &recipe, &[], 100, None, 0).expect("plan");
        assert_eq!(
            plan.effects,
            vec![Effect::UnlockBlueprint {
                blueprint_key: "nd_furnace".to_string(),
                target_card_id: 100,
            }]
        );
    }

    #[test]
    fn plan_output_rejects_faction_op() {
        // `aspect.faction.set` is still deferred.
        let recipe = Recipe {
            id: "t".to_string(),
            input: vec![],
            output: vec![stmt(
                vec![word("root"), word("aspect"), word("faction"), word("set")],
                Some(StatementValue::Int(1)),
            )],
            iterators: vec![],
            anchors: AnchorSet {
                root: true,
                branches: 0,
            },
            root_slot_hold: true,
            root_position_hold: false,
        };
        let store = Mock(HashMap::new());
        let err = plan_output(&store, &recipe, &[], 100, None, 0).unwrap_err();
        assert!(err.contains("faction"), "{err}");
    }

    fn recipe_with(
        anchor_root: bool,
        root_slot_hold: bool,
        root_position_hold: bool,
        iters: Vec<Iterator>,
    ) -> Recipe {
        Recipe {
            id: "test".to_string(),
            input: vec![],
            output: vec![],
            iterators: iters,
            anchors: AnchorSet {
                root: anchor_root,
                branches: 0,
            },
            root_slot_hold,
            root_position_hold,
        }
    }

    fn iter(slot_hold: bool, position_hold: bool) -> Iterator {
        Iterator {
            parent: vec![],
            branch: 1,
            slot_hold,
            position_hold,
        }
    }

    #[test]
    fn synthetic_tile_holds_only_when_tile_bound() {
        // Iterator 0 bound to the sentinel-`0` tile, `use`/`claim`-style → slot_hold.
        let r = recipe_with(false, true, false, vec![iter(true, false)]);
        let h = synthetic_tile_holds(&r, &[vec![0]]).expect("tile bound");
        assert!(h.slot_hold && !h.slot_share);

        // `share`/`borrow`-style tile slot (slot_hold=false) → slot_share (+ pin).
        let rs = recipe_with(false, true, false, vec![iter(false, true)]);
        let hs = synthetic_tile_holds(&rs, &[vec![0]]).expect("tile bound");
        assert!(hs.slot_share && hs.position_hold && !hs.slot_hold);

        // No sentinel-`0` binding (root-only recipe like `corpus-`) → None, so the
        // gateway never promotes a spurious tile on the cell the card sits on.
        let no_tile = recipe_with(true, true, false, vec![]);
        assert!(synthetic_tile_holds(&no_tile, &[]).is_none());
        // ...and not even when a real-card iterator is present (no `0` in it).
        let card_only = recipe_with(false, true, false, vec![iter(true, false)]);
        assert!(synthetic_tile_holds(&card_only, &[vec![1027]]).is_none());
    }

    #[test]
    fn root_share_and_iterator_claim() {
        // root anchored, share (not slot_hold) + position-pinned;
        // one iterator that claims (slot_hold) but doesn't pin.
        let recipe = recipe_with(true, false, true, vec![iter(true, false)]);
        let holds = compute_holds(&recipe, &[vec![50]], 100);

        let root = holds.get(&100).copied().unwrap();
        assert_eq!(
            root,
            HoldKinds {
                slot_hold: false,
                position_hold: true,
                slot_share: true,
            }
        );
        let bound = holds.get(&50).copied().unwrap();
        assert_eq!(
            bound,
            HoldKinds {
                slot_hold: true,
                position_hold: false,
                slot_share: false,
            }
        );
    }

    #[test]
    fn sentinel_and_unanchored_root_skipped() {
        // root not anchored → no root entry; the `0` sentinel is skipped.
        let recipe = recipe_with(false, false, false, vec![iter(false, true)]);
        let holds = compute_holds(&recipe, &[vec![0, 77]], 100);
        assert!(!holds.contains_key(&100));
        assert!(!holds.contains_key(&0));
        let bound = holds.get(&77).copied().unwrap();
        assert_eq!(
            bound,
            HoldKinds {
                slot_hold: false,
                position_hold: true,
                slot_share: true,
            }
        );
    }
}
