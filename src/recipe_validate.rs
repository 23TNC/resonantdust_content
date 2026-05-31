//! Storage-agnostic recipe **input validation** — does a bound stack satisfy a
//! recipe's input predicates?
//!
//! This is the predicate half of the legacy `shard::actions::propose_action`
//! verifier (`verify_input` → `verify_stmt` → `resolve_target` / `aspect_total`),
//! lifted out of `ReducerContext` so the gateway can run it over its gathered
//! snapshot. Card reads go through the [`CardStore`] trait; everything else —
//! the recipe registry, definition decoding, aspect hierarchy, flag layout — is
//! the same `content` machinery both the modules and the gateway already share.
//!
//! Scope: this validates recipe **shape** (the bound cards match the input
//! predicates). Card **state** checks (not-dead / not-held / ownership /
//! magnetic discipline — the legacy `validate_bindings`) stay on the DB side:
//! the per-shard hold/dedup reducers the gateway calls are the race guard.

use crate::definition_core::{
    aspect_id, decode_definition, is_aspect_descendant, lifecycle_recipe_for_def, AspectId,
};
use crate::flags_core::{flag_bit, flag_field};
use crate::packed::unpack_definition;
use crate::recipe_core::{Recipe, Seg, Stmt};
use crate::recipe_statement::StatementValue;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// `card_type` of the tile-as-card family (zone-tile cards carry per-row
/// stock in `flags_bk`). Mirrors the constant duplicated across the shard.
const TILE_CARD_TYPE: u8 = 7;

/// The card fields recipe validation reads. Implementers fill these from their
/// storage — the gateway from its gathered snapshot, tests from a mock.
#[derive(Clone, Debug)]
pub struct CardView {
    pub card_id: u32,
    pub owner_id: u32,
    pub micro_location: u32,
    pub macro_zone: u64,
    pub packed_definition: u16,
    pub flags_state: u32,
    pub flags_bk: u32,
}

/// Point-in-time card reads. The gateway implements this over its snapshot;
/// `shard` keeps its own `ReducerContext`-based reads.
pub trait CardStore {
    /// The card's state as of `time_ms` (latest version ≤ time), or `None`.
    fn card_at(&self, card_id: u32, time_ms: u64) -> Option<CardView>;
}

/// A synthetic tile substituted for the branch-0 sentinel (`0`): its packed
/// definition plus `(stock0, stock1)`.
pub type SyntheticTile = (u16, (u8, u8));

// ---- flags_bk layout (from content/cards/flags.json) -------------------

struct BkLayout {
    micro_is_card_mask: u32,
    stack_state_mask: u32,
    stack_state_shift: u8,
    tile_stock_mask: [u32; 2],
    tile_stock_shift: [u8; 2],
    /// Refcount hold fields — `(mask, shift)` each. Read as `count > 0` gates.
    slot_hold_count: (u32, u8),
    slot_share_count: (u32, u8),
    drop_hold_count: (u32, u8),
    touch_count: (u32, u8),
}

fn bk_layout() -> &'static BkLayout {
    static L: OnceLock<BkLayout> = OnceLock::new();
    L.get_or_init(|| {
        let bit = |n: &str| {
            1u32 << flag_bit("cards_bk", n)
                .ok()
                .flatten()
                .unwrap_or_else(|| panic!("cards/flags.json: missing bk bit {n:?}"))
        };
        let field = |n: &str| {
            flag_field("cards_bk", n)
                .ok()
                .flatten()
                .unwrap_or_else(|| panic!("cards/flags.json: missing bk field {n:?}"))
        };
        let ss = field("stack_state");
        let t0 = field("tile_stock_0");
        let t1 = field("tile_stock_1");
        let cf = |n: &str| {
            let f = field(n);
            (f.mask(), f.shift)
        };
        BkLayout {
            micro_is_card_mask: bit("micro_is_card"),
            stack_state_mask: ss.mask(),
            stack_state_shift: ss.shift,
            tile_stock_mask: [t0.mask(), t1.mask()],
            tile_stock_shift: [t0.shift, t1.shift],
            slot_hold_count: cf("slot_hold_count"),
            slot_share_count: cf("slot_share_count"),
            drop_hold_count: cf("drop_hold_count"),
            touch_count: cf("touch_count"),
        }
    })
}

/// `cards_state` bits the binding validator gates on.
struct StateLayout {
    dead: u32,
    magnetic: u32,
    is_owned_by_player: u32,
}

fn state_layout() -> &'static StateLayout {
    static L: OnceLock<StateLayout> = OnceLock::new();
    L.get_or_init(|| {
        let bit = |n: &str| {
            1u32 << flag_bit("cards_state", n)
                .ok()
                .flatten()
                .unwrap_or_else(|| panic!("cards/flags.json: missing state bit {n:?}"))
        };
        StateLayout {
            dead: bit("dead"),
            magnetic: bit("magnetic"),
            is_owned_by_player: bit("is_owned_by_player"),
        }
    })
}

fn count_of(flags_bk: u32, (mask, shift): (u32, u8)) -> u32 {
    (flags_bk & mask) >> shift
}

/// True when `micro_location` is a root card_id (the card is a stack member).
pub(crate) fn micro_is_card(view: &CardView) -> bool {
    view.flags_bk & bk_layout().micro_is_card_mask != 0
}

/// The `stack_state` branch value (gated on [`micro_is_card`]).
fn stack_branch(view: &CardView) -> u8 {
    let l = bk_layout();
    ((view.flags_bk & l.stack_state_mask) >> l.stack_state_shift) as u8
}

/// Read tile-card per-row stock `slot` (0 or 1) from `flags_bk`.
fn tile_stock(flags_bk: u32, slot: usize) -> u8 {
    let l = bk_layout();
    ((flags_bk & l.tile_stock_mask[slot]) >> l.tile_stock_shift[slot]) as u8
}

// ---- public entry ------------------------------------------------------

/// Validate every input predicate of `recipe` against the bound stack. `Ok(())`
/// means the stack satisfies the recipe; `Err` names the failing predicate.
pub fn validate_input<S: CardStore>(
    store: &S,
    recipe: &Recipe,
    root: u32,
    bindings: &[Vec<u32>],
    synthetic: Option<SyntheticTile>,
    now_ms: u64,
) -> Result<(), String> {
    for (i, stmt) in recipe.input.iter().enumerate() {
        verify_stmt(store, recipe, stmt, root, bindings, synthetic, now_ms)
            .map_err(|e| format!("input[{i}]: {e}"))?;
    }
    Ok(())
}

/// World-anonymous player id (cards whose owner chain bottoms out at 0).
pub const WORLD_PLAYER_ID: u32 = 0;
const OWNER_WALK_DEPTH_CAP: u32 = 32;
const TOUCH_COUNT_CLIENT_CAP: u32 = 3;

/// Walk a card's `owner_id` chain to the responsible player. A card carrying
/// `is_owned_by_player` names its player directly in `owner_id`; otherwise the
/// owner is another card and we recurse. A chain that bottoms out at owner 0
/// resolves to [`WORLD_PLAYER_ID`]. Port of `shard::cards::owning_player` over
/// [`CardStore`]. `None` only on a missing row or a cycle past the depth cap.
pub fn owning_player<S: CardStore>(store: &S, card_id: u32, now_ms: u64) -> Option<u32> {
    let st = state_layout();
    let mut cur = card_id;
    for _ in 0..OWNER_WALK_DEPTH_CAP {
        let row = store.card_at(cur, now_ms)?;
        if row.flags_state & st.is_owned_by_player != 0 {
            return Some(row.owner_id);
        }
        if row.owner_id == 0 {
            return Some(WORLD_PLAYER_ID);
        }
        cur = row.owner_id;
    }
    None
}

/// Stack-vs-world validation: every bound card exists, is not dead, is not
/// held by a conflicting in-flight action, is owned by the caller (or world),
/// appears only once, and — if magnetic — is locked to this `recipe_id`. Port
/// of `shard::actions::validate_bindings` over [`CardStore`].
///
/// NB: over a gathered snapshot the hold-count gates are best-effort (a TOCTOU
/// window exists vs. concurrent gateways); the per-shard dedup reducer
/// (`claim_pending`) is the exact-duplicate guard.
pub fn validate_bindings<S: CardStore>(
    store: &S,
    recipe: &Recipe,
    recipe_id: u16,
    root: u32,
    bindings: &[Vec<u32>],
    caller_player_id: u32,
    now_ms: u64,
) -> Result<(), String> {
    let bk = bk_layout();
    let st = state_layout();

    // Root: dead + hold-kind + touch + drop gates.
    if root != 0 {
        let card = store
            .card_at(root, now_ms)
            .ok_or_else(|| format!("root card {root} not found"))?;
        if card.flags_state & st.dead != 0 {
            return Err(format!("root card {root} is dead"));
        }
        if count_of(card.flags_bk, bk.slot_hold_count) > 0 {
            return Err(format!(
                "root card {root} is exclusively held by another in-flight action"
            ));
        }
        if recipe.root_slot_hold && count_of(card.flags_bk, bk.slot_share_count) > 0 {
            return Err(format!(
                "root card {root} is shared-held by another in-flight action; cannot claim"
            ));
        }
        if count_of(card.flags_bk, bk.touch_count) >= TOUCH_COUNT_CLIENT_CAP {
            return Err(format!(
                "root card {root} has too many concurrent in-flight actions (cap {TOUCH_COUNT_CLIENT_CAP})"
            ));
        }
        if count_of(card.flags_bk, bk.drop_hold_count) > 0 {
            return Err(format!(
                "root card {root} blocks stacking (drop_hold_count > 0)"
            ));
        }
    }

    let mut seen: BTreeSet<u32> = BTreeSet::new();
    for (iter_id, binding_row) in bindings.iter().enumerate() {
        let iter_locks = recipe
            .iterators
            .get(iter_id)
            .map(|it| it.slot_hold)
            .unwrap_or(true);
        for &card_id in binding_row.iter() {
            if card_id == 0 {
                continue;
            }
            if !seen.insert(card_id) {
                return Err(format!("card {card_id} appears more than once in bindings"));
            }
            let card = store
                .card_at(card_id, now_ms)
                .ok_or_else(|| format!("card {card_id} not found"))?;
            if card.flags_state & st.dead != 0 {
                return Err(format!("card {card_id} is dead"));
            }
            if count_of(card.flags_bk, bk.slot_hold_count) > 0 {
                return Err(format!(
                    "card {card_id} is exclusively held by another in-flight action"
                ));
            }
            if iter_locks && count_of(card.flags_bk, bk.slot_share_count) > 0 {
                return Err(format!(
                    "card {card_id} is shared-held by another in-flight action; cannot claim"
                ));
            }
            if count_of(card.flags_bk, bk.touch_count) >= TOUCH_COUNT_CLIENT_CAP {
                return Err(format!(
                    "card {card_id} has too many concurrent in-flight actions (cap {TOUCH_COUNT_CLIENT_CAP})"
                ));
            }
            let owner_player = owning_player(store, card_id, now_ms).unwrap_or(WORLD_PLAYER_ID);
            if owner_player != caller_player_id && owner_player != WORLD_PLAYER_ID {
                return Err(format!(
                    "card {card_id} is owned by player {owner_player}, not caller {caller_player_id}"
                ));
            }
            if card.flags_state & st.magnetic != 0 {
                let def = decode_definition(card.packed_definition)
                    .map_err(|e| format!("decode def for magnetic check: {e}"))?
                    .ok_or_else(|| format!("card {card_id} has unknown def"))?;
                let expected = lifecycle_recipe_for_def(def)
                    .map_err(|e| format!("magnetic recipe lookup: {e}"))?
                    .ok_or_else(|| {
                        format!(
                            "card {card_id} carries magnetic flag but def declares no magnetic recipe"
                        )
                    })?;
                if expected != recipe_id {
                    return Err(format!(
                        "card {card_id} is magnetic-locked to recipe {expected}, got {recipe_id}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Evaluate one input predicate. Path-first grammar — the last segment is the
/// op: `<path>.def_id: <key>` or `<path>.aspect.<name>.min: <N>`.
fn verify_stmt<S: CardStore>(
    store: &S,
    recipe: &Recipe,
    stmt: &Stmt,
    root: u32,
    bindings: &[Vec<u32>],
    synthetic: Option<SyntheticTile>,
    now_ms: u64,
) -> Result<(), String> {
    let segs = stmt.segments.as_slice();
    let op = segs
        .last()
        .and_then(|s| match s {
            Seg::Word(w) => Some(w.as_str()),
            _ => None,
        })
        .ok_or_else(|| "empty path or non-word terminal segment".to_string())?;

    match op {
        "def_id" => {
            let key = match &stmt.value {
                Some(StatementValue::Str(s)) => s.as_str(),
                _ => return Err("def_id: requires a string value".to_string()),
            };
            let target = &segs[..segs.len() - 1];
            let (packed_def, _stocks) =
                resolve_target(store, recipe, target, root, bindings, synthetic, now_ms)?;
            let def = decode_definition(packed_def)
                .map_err(|e| format!("decode card def: {e}"))?
                .ok_or_else(|| format!("def_id check: packed {packed_def:#06x} has no def"))?;
            if def.key != key {
                return Err(format!("def_id: expected {key:?}, got {:?}", def.key));
            }
            Ok(())
        }
        "min" => {
            // <path>.aspect.<name>.min: <N>
            if segs.len() < 4 {
                return Err(format!(
                    "min predicate expects `<path>.aspect.<name>.min`, got {segs:?}"
                ));
            }
            match &segs[segs.len() - 3] {
                Seg::Word(w) if w == "aspect" => {}
                other => {
                    return Err(format!(
                        "min predicate expects `aspect` before name; got {other:?}"
                    ))
                }
            }
            let aspect_name = match &segs[segs.len() - 2] {
                Seg::Word(w) => w.as_str(),
                other => {
                    return Err(format!("min predicate expects an aspect name; got {other:?}"))
                }
            };
            let min_value = match &stmt.value {
                Some(StatementValue::Int(n)) => *n as i32,
                _ => return Err("min: requires an integer value".to_string()),
            };
            let target = &segs[..segs.len() - 3];
            let aspect = aspect_id(aspect_name)
                .map_err(|e| format!("aspect lookup: {e}"))?
                .ok_or_else(|| format!("unknown aspect {aspect_name:?}"))?;
            let (packed_def, stocks) =
                resolve_target(store, recipe, target, root, bindings, synthetic, now_ms)?;
            let total = aspect_total(packed_def, aspect, stocks)?;
            if total < min_value {
                return Err(format!(
                    "aspect {aspect_name:?}.min: required >= {min_value}, got {total}"
                ));
            }
            Ok(())
        }
        "destroy" | "create" | "set" | "add" | "sub" | "random" => {
            Err(format!("{op:?} is an output op; not valid in input statements"))
        }
        "gt" | "ge" | "lt" | "le" | "eq" | "ne" => Err(format!(
            "{op:?} comparison ops are output-side gates; input predicates use `min` or `def_id`"
        )),
        other => Err(format!("unsupported predicate op {other:?}")),
    }
}

/// Sum aspect values visible to the predicate matcher. Per-row stocks (when
/// present and matching) take precedence over static aspects.
fn aspect_total(packed_def: u16, aspect: AspectId, stocks: Option<(u8, u8)>) -> Result<i32, String> {
    let def = decode_definition(packed_def)
        .map_err(|e| format!("decode def: {e}"))?
        .ok_or_else(|| format!("packed {packed_def:#06x} has no def"))?;
    if let Some((s0, s1)) = stocks {
        let mut had_match = false;
        let mut stock_total: i32 = 0;
        for (idx, slot) in def.stock.iter().enumerate() {
            if is_aspect_descendant(slot.aspect_id, aspect).unwrap_or(false) {
                had_match = true;
                stock_total += if idx == 0 { s0 } else { s1 } as i32;
            }
        }
        if had_match {
            return Ok(stock_total);
        }
    }
    let total: i32 = def
        .aspects
        .iter()
        .filter(|(a, _)| is_aspect_descendant(*a, aspect).unwrap_or(false))
        .map(|(_, v)| *v as i32)
        .sum();
    Ok(total)
}

/// Resolve a segment path to the target's `(packed_definition, stocks)`. Walks
/// the anchor + slot refs + `.owner` / `.parent` chain steps, confirming the
/// relationships the path claims actually hold. O(1) per segment.
fn resolve_target<S: CardStore>(
    store: &S,
    recipe: &Recipe,
    path: &[Seg],
    root: u32,
    bindings: &[Vec<u32>],
    synthetic: Option<SyntheticTile>,
    now_ms: u64,
) -> Result<(u16, Option<(u8, u8)>), String> {
    // First segment → a card_id: `root` or a top-level `Slot` reference.
    let mut card_id = match path.first() {
        Some(Seg::Word(w)) if w == "root" => {
            if root == 0 {
                return Err("root anchor: root is 0".to_string());
            }
            root
        }
        Some(Seg::Slot { iterator_id, offset }) => {
            let it = recipe
                .iterators
                .get(*iterator_id as usize)
                .ok_or_else(|| format!("iterator_id {iterator_id} out of range"))?;
            let binding_row = bindings
                .get(*iterator_id as usize)
                .ok_or_else(|| format!("bindings missing entry for iterator {iterator_id}"))?;
            let resolved = binding_row.get(*offset as usize).copied().ok_or_else(|| {
                format!(
                    "iterator {iterator_id} offset {offset} out of range (binding len {})",
                    binding_row.len()
                )
            })?;
            // Synthetic-tile case: branch 0, top-level, sentinel 0, leaf only.
            if resolved == 0 && it.parent.is_empty() && it.branch == 0 && *offset == 0 {
                if let Some((packed, stocks)) = synthetic {
                    if path.len() == 1 {
                        return Ok((packed, Some(stocks)));
                    }
                    return Err(
                        "synthetic tile doesn't support owner/parent chain navigation".to_string(),
                    );
                }
            }
            if resolved == 0 {
                return Err(format!(
                    "iterator {iterator_id} offset {offset}: binding is 0 (no-card sentinel)"
                ));
            }
            resolved
        }
        Some(other) => return Err(format!("unsupported top-level anchor segment {other:?}")),
        None => return Err("empty path".to_string()),
    };

    // Walk subsequent segments, tracking the .owner/.parent transition.
    enum Expect {
        Anything,
        Stepped, // immediately after an .owner / .parent step
    }
    let mut expect = Expect::Anything;
    let mut i = 1;
    while i < path.len() {
        match &path[i] {
            Seg::Word(w) if w == "owner" => {
                let card = store
                    .card_at(card_id, now_ms)
                    .ok_or_else(|| format!("card {card_id} not found"))?;
                if card.owner_id == 0 {
                    return Err(format!("owner step: card {card_id} has no owner"));
                }
                card_id = card.owner_id;
                expect = Expect::Stepped;
                i += 1;
            }
            Seg::Word(w) if w == "parent" => {
                let card = store
                    .card_at(card_id, now_ms)
                    .ok_or_else(|| format!("card {card_id} not found"))?;
                if !micro_is_card(&card) {
                    return Err(format!(
                        "parent step: card {card_id} has no parent (it is a chain root)"
                    ));
                }
                card_id = card.micro_location;
                expect = Expect::Stepped;
                i += 1;
            }
            Seg::Slot { iterator_id, offset } => {
                let it = recipe
                    .iterators
                    .get(*iterator_id as usize)
                    .ok_or_else(|| format!("iterator_id {iterator_id} out of range"))?;
                let binding_row = bindings
                    .get(*iterator_id as usize)
                    .ok_or_else(|| format!("bindings missing entry for iterator {iterator_id}"))?;
                let resolved = binding_row.get(*offset as usize).copied().ok_or_else(|| {
                    format!("iterator {iterator_id} offset {offset} out of range (binding len {})", binding_row.len())
                })?;
                if resolved == 0 {
                    return Err(format!("iterator {iterator_id} offset {offset}: binding is 0"));
                }
                // Reject a Slot-after-Slot with no .owner/.parent between —
                // a malformed path. (The branch/ownership/hold checks live in
                // the DB-side state validation.)
                if matches!(expect, Expect::Anything) && i > 1 {
                    if let Seg::Slot { .. } = &path[i - 1] {
                        return Err(format!(
                            "unexpected slot reference (iter {iterator_id}, offset {offset}) without prior owner/parent step"
                        ));
                    }
                }
                // Nested iterators: the bound card must be in the right branch.
                if !it.parent.is_empty() {
                    let card = store
                        .card_at(resolved, now_ms)
                        .ok_or_else(|| format!("card {resolved} not found"))?;
                    let actual_dir = stack_branch(&card);
                    if actual_dir != it.branch {
                        return Err(format!(
                            "branch mismatch: iterator {iterator_id} expects branch {}, but card {resolved}'s actual direction is {actual_dir}",
                            it.branch
                        ));
                    }
                }
                card_id = resolved;
                expect = Expect::Anything;
                i += 1;
            }
            other => {
                return Err(format!("unsupported path segment {other:?} in chain navigation"))
            }
        }
    }

    let card = store
        .card_at(card_id, now_ms)
        .ok_or_else(|| format!("resolve target: card {card_id} not found"))?;
    // Tile-cards surface per-row stock from flags_bk; non-tiles report None.
    let (card_type, _) = unpack_definition(card.packed_definition);
    let stocks = if card_type == TILE_CARD_TYPE {
        Some((tile_stock(card.flags_bk, 0), tile_stock(card.flags_bk, 1)))
    } else {
        None
    };
    Ok((card.packed_definition, stocks))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a flags_bk value with the given fields set, using the real layout
    // from cards/flags.json — exercises the bit-op plumbing the port depends on.
    fn flags_bk_with(micro_is_card_set: bool, stack_state: u8, stock0: u8, stock1: u8) -> u32 {
        let l = bk_layout();
        let mut bk = 0u32;
        if micro_is_card_set {
            bk |= l.micro_is_card_mask;
        }
        bk |= ((stack_state as u32) << l.stack_state_shift) & l.stack_state_mask;
        bk |= ((stock0 as u32) << l.tile_stock_shift[0]) & l.tile_stock_mask[0];
        bk |= ((stock1 as u32) << l.tile_stock_shift[1]) & l.tile_stock_mask[1];
        bk
    }

    #[test]
    fn flag_bitops_roundtrip() {
        let bk = flags_bk_with(true, 2, 3, 1);
        let view = CardView {
            card_id: 1,
            owner_id: 0,
            micro_location: 0,
            macro_zone: 0,
            packed_definition: 0,
            flags_state: 0,
            flags_bk: bk,
        };
        assert!(micro_is_card(&view));
        assert_eq!(stack_branch(&view), 2);
        assert_eq!(tile_stock(bk, 0), 3);
        assert_eq!(tile_stock(bk, 1), 1);

        let clear = flags_bk_with(false, 0, 0, 0);
        let view2 = CardView { flags_bk: clear, ..view };
        assert!(!micro_is_card(&view2));
        assert_eq!(stack_branch(&view2), 0);
    }

    use crate::recipe_tape::{AnchorSet, Recipe};
    use std::collections::HashMap;

    struct Mock(HashMap<u32, CardView>);
    impl CardStore for Mock {
        fn card_at(&self, id: u32, _t: u64) -> Option<CardView> {
            self.0.get(&id).cloned()
        }
    }
    fn card(id: u32, owner: u32, flags_state: u32, flags_bk: u32) -> CardView {
        CardView {
            card_id: id,
            owner_id: owner,
            micro_location: 0,
            macro_zone: 0,
            packed_definition: 0,
            flags_state,
            flags_bk,
        }
    }
    fn empty_recipe() -> Recipe {
        Recipe {
            id: "t".to_string(),
            input: vec![],
            output: vec![],
            iterators: vec![],
            anchors: AnchorSet {
                root: false,
                branches: 0,
            },
            root_slot_hold: false,
            root_position_hold: false,
        }
    }

    #[test]
    fn bindings_world_owned_ok() {
        // owner_id 0 → resolves to WORLD_PLAYER_ID, allowed for any caller.
        let mut m = HashMap::new();
        m.insert(50, card(50, 0, 0, 0));
        let store = Mock(m);
        validate_bindings(&store, &empty_recipe(), 1, 0, &[vec![50]], 7, 0).expect("ok");
    }

    #[test]
    fn bindings_dup_rejected() {
        let mut m = HashMap::new();
        m.insert(50, card(50, 0, 0, 0));
        let store = Mock(m);
        let err =
            validate_bindings(&store, &empty_recipe(), 1, 0, &[vec![50, 50]], 7, 0).unwrap_err();
        assert!(err.contains("more than once"), "{err}");
    }

    #[test]
    fn bindings_dead_rejected() {
        let st = state_layout();
        let mut m = HashMap::new();
        m.insert(50, card(50, 0, st.dead, 0));
        let store = Mock(m);
        let err = validate_bindings(&store, &empty_recipe(), 1, 0, &[vec![50]], 7, 0).unwrap_err();
        assert!(err.contains("is dead"), "{err}");
    }

    #[test]
    fn bindings_foreign_owner_rejected() {
        // card owned by player 9 (is_owned_by_player set, owner_id = 9), caller 7.
        let st = state_layout();
        let mut m = HashMap::new();
        m.insert(50, card(50, 9, st.is_owned_by_player, 0));
        let store = Mock(m);
        let err = validate_bindings(&store, &empty_recipe(), 1, 0, &[vec![50]], 7, 0).unwrap_err();
        assert!(err.contains("owned by player 9"), "{err}");
    }
}
