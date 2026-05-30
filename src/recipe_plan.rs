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

use crate::recipe_core::Recipe;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe_tape::{AnchorSet, Iterator, Recipe};

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
