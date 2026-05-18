// Pack/unpack helpers for the bit-packed columns on the cards table.
// Single source of truth — the spacetime shard / chat modules
// re-export this module verbatim via `pub use
// resonantdust_content::packed::*;`.
//
// Layouts:
//   valid_at         u64 = [time_ms: u48 | sequence: u16]                   (high | low)
//                          — see `pack_valid_at` below and `sequence.rs`
//                          for how the u16 disambiguator is allocated.
//   macro_zone       u32 = [q: i16 | r: i16]                                (high | low)
//   micro_zone       u8  — TWO INTERPRETATIONS, gated by (stacked_state, surface):
//     stack layout — state == OnRoot AND surface < WORLD_LAYER:
//                   u8 = [position: u4 | direction: u2 | stacked_state: u2]
//                   direction: 0 = up / top, 1 = down / bottom, 2 = hex.
//     legacy layout — Free state (and any reserved/state-3 rows):
//                   u8 = [q: u3 | r: u3 | stacked_state: u2]
//   micro_location   u32 — interpretation depends on stacked_state:
//     Free          → [x: i16 | y: i16] (loose XY in surface-local coords)
//     Slot          → immediate parent's card_id (chain walks up via this).
//     OnRoot        → ROOT card_id of the chain (chain order / direction
//                     from `micro_zone.position` and `micro_zone.direction`).
//   packed_def       u16 = [card_type: u4 | def_id: u12]
//   zone_def         u8  = [card_type: u4 | 0: u4] (lower nibble reserved)
//   tile slot        u16 = [def_id: u12 | stock0: u2 | stock1: u2]
//                          packed 4-per-u64 across 16 u64s per zone — see
//                          docs/TILE_ASPECTS.md.
//
// Unified card model: every card is a card. The branching off a root
// is purely directional — `direction = 0/1/2` selects which side of
// the root (top / bottom / hex) the child chains onto. The card's
// `card_type` only drives shape rendering, not structural logic. No
// special "hex-anchored" state exists anymore (it was `OnHex = 3` in
// the legacy model and is retired in the unified model).
//
// The "server is forcing this position" signal moved out of micro_zone
// (it used to live in bit 2 alongside position/state) and now lives in
// `flags` (`force_position` at bit 11) — see `cards/flags.json`.

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackedState {
    /// Loose. `micro_location` is encoded `(x, y)`; `micro_zone` upper
    /// 6 bits are the legacy `(q, r)`. Roots of in-flight chains and
    /// every world-surface card use this state.
    Free = 0,
    /// Parent-pointer slot. `micro_location` holds the **immediate
    /// parent's** card_id (which can itself be `Slot`, `OnRoot`, or
    /// `Free`). The slot's direction (bits 2-3 of `micro_zone`) is
    /// stored explicitly even though it could be derived from the
    /// parent — saves a chain walk on every render / parent-resolve.
    /// Position from root is implicit: walk `micro_location` up until
    /// you hit a state that's not `Slot` (that'll be `OnRoot` or
    /// `Free`). Used by `propose_action` to stitch slots above the
    /// root in any of the three branches.
    Slot = 1,
    /// Stacked on a chain root — `micro_location` is the **root**
    /// card_id (not the immediate parent). Chain order comes from
    /// `micro_zone.position`; chain branch (top / bottom / hex) comes
    /// from `micro_zone.direction`.
    OnRoot = 2,
    // Value 3 is reserved — was `OnHex` in the legacy hex-anchored
    // model; retired with the unified card model.
}

impl StackedState {
    /// Decode a 2-bit state value. Value 3 panics — it was the legacy
    /// `OnHex` variant; under the unified card model no card row
    /// should carry that state. Encountering one indicates either
    /// stale data from before the migration or a bit-packing bug.
    pub fn from_u2(v: u8) -> Self {
        match v & 0b11 {
            0 => Self::Free,
            1 => Self::Slot,
            2 => Self::OnRoot,
            3 => panic!("StackedState::from_u2: value 3 is reserved (legacy OnHex)"),
            _ => unreachable!(),
        }
    }

    pub fn to_u2(self) -> u8 {
        self as u8
    }
}

// ---- surface bands ------------------------------------------------------
//
// `Card.surface` and `Zone.surface` are u8, but the values are
// banded into ranges with different semantics. Every band is a
// "container kind" — the `(surface, macro_zone)` tuple identifies
// which container a row belongs to. `macro_zone` means different
// things in different bands:
//
// - INVENTORY_LAYER (1):           `macro_zone` = the owning soul's
//                                  `card_id`. The player's hand /
//                                  bag / inventory grid.
// - POCKET_DIMENSION_LAYER (32):   `macro_zone` = the anchor card's
//                                  `card_id`. A private interior
//                                  carried by an anchor card.
// - MINI_ZONE_LAYER (63):          `macro_zone` = the anchor card's
//                                  `card_id`. A radius-3 hex disk
//                                  overlaying the world wherever
//                                  the anchor is placed. The anchor
//                                  itself lives at WORLD_LAYER.
// - WORLD_LAYER (64) and above:    `macro_zone` = packed
//                                  `(chunkQ:i16, chunkR:i16)`. The
//                                  shared world hex grid.
//
// The split at `< WORLD_LAYER` is what existing code keys "stack
// layout" rules and inventory-like behavior off. The split at
// `>= WORLD_LAYER` is what world-vs-personal queries key off. The
// MINI_ZONE_LAYER sits just below WORLD_LAYER intentionally: stack-
// layout rules apply (cards on mini_zone tiles can chain with the
// existing rect-stack machinery), and world-only queries continue
// to skip mini_zone contents.
pub const INVENTORY_LAYER: u8 = 1;
pub const POCKET_DIMENSION_LAYER: u8 = 32;
pub const MINI_ZONE_LAYER: u8 = 63;
pub const WORLD_LAYER: u8 = 64;

// ---- valid_at ----------------------------------------------------------
//
// PK layout: `(time_ms_u48 << 16) | sequence_u16`.
//
// - High 48 bits: milliseconds since Unix epoch. u48 ms ≈ 8920 years
//   of runway from epoch (covers our lifetime trivially). PK ordering
//   is chronological — a btree scan walks rows in time order, useful
//   for any range queries that need it (though most callers go via
//   `card_id` btree index, where the explicit `max_by_key` ordering
//   is what's load-bearing).
// - Low 16 bits: global sequence number from `sequence::next_sequence`,
//   refreshed per write. Disambiguates two writes that share a
//   millisecond — within one module, even thousands of same-ms writes
//   sit far below the 65k wrap budget. Cross-shard collisions don't
//   exist because shards' PK spaces don't overlap (each shard owns
//   its own rows entirely).

pub fn pack_valid_at(time_ms: u64, sequence: u16) -> u64 {
    (time_ms << 16) | (sequence as u64)
}

pub fn valid_at_time(v: u64) -> u64 {
    v >> 16
}

// ---- macro_zone --------------------------------------------------------

pub fn pack_macro_zone(q: i16, r: i16) -> u32 {
    ((q as u16 as u32) << 16) | (r as u16 as u32)
}

pub fn unpack_macro_zone(v: u32) -> (i16, i16) {
    ((v >> 16) as u16 as i16, v as u16 as i16)
}

// ---- micro_zone --------------------------------------------------------

pub fn pack_micro_zone(q: u8, r: u8, state: StackedState) -> u8 {
    ((q & 0b111) << 5) | ((r & 0b111) << 2) | state.to_u2()
}

pub fn unpack_micro_zone(v: u8) -> (u8, u8, StackedState) {
    (
        (v >> 5) & 0b111,
        (v >> 2) & 0b111,
        StackedState::from_u2(v),
    )
}

pub fn micro_zone_state(v: u8) -> StackedState {
    StackedState::from_u2(v)
}

/// Whether the **stack layout** applies to this `(state, surface)` pair.
/// True when the card is stacked on inventory (state == OnRoot AND
/// surface < WORLD_LAYER). False for Free, Slot, and world surfaces —
/// Free keeps the legacy `(q, r)` layout; Slot reads through
/// [`unpack_stack_micro_zone`] for the direction bits but doesn't go
/// through the mirror's preserve gate.
pub fn is_stack_layout(state: StackedState, surface: u8) -> bool {
    surface < WORLD_LAYER && matches!(state, StackedState::OnRoot)
}

/// Direction values within the stack layout. Two bits — values 0, 1,
/// 2 are valid; value 3 is reserved.
///
/// - `0 = up / top` — child chains stacked above root.
/// - `1 = down / bottom` — child chains stacked below root.
/// - `2 = hex` — child chains stacked into the hex branch.
///
/// The three branches are structurally identical (parent-pointer chain
/// in `Slot` state, root-anchored chain in `OnRoot` state). The
/// direction value only tells client renderers which side of root to
/// visually attach the chain to.
pub const STACK_DIR_UP: u8 = 0;
pub const STACK_DIR_DOWN: u8 = 1;
pub const STACK_DIR_HEX: u8 = 2;

/// Pack `(position, direction, state)` under the stack layout:
/// `[position: u4 | direction: u2 | stacked_state: u2]`.
///
/// `position` is the card's index in its chain from the root (1..=15;
/// 0 reserved for "no chain"). Saturates at `0b1111` if higher — chains
/// deeper than 15 cards are rejected at propose-time by `actions.rs`.
/// `direction` is one of [`STACK_DIR_UP`] / [`STACK_DIR_DOWN`] /
/// [`STACK_DIR_HEX`]; value 3 is reserved.
///
/// The "server is forcing this position" signal moved out of micro_zone
/// (it used to share bits with state/direction) and lives in `flags`
/// (`force_position` at bit 11).
///
/// **Only valid for `state == OnRoot`.** Free cards use
/// [`pack_micro_zone`]; Slot cards use [`pack_slot_micro_zone`].
pub fn pack_stack_micro_zone(position: u8, direction: u8, state: StackedState) -> u8 {
    debug_assert!(
        matches!(state, StackedState::OnRoot),
        "pack_stack_micro_zone only valid for OnRoot; got {state:?}",
    );
    let pos = position & 0b1111;
    let dir = direction & 0b11;
    (pos << 4) | (dir << 2) | state.to_u2()
}

/// Inverse of [`pack_stack_micro_zone`]. Returns `(position, direction, state)`.
/// The caller is responsible for knowing the byte was packed under the stack
/// layout; reading a legacy-layout byte through here gives nonsense for the
/// `position` and `direction` fields.
pub fn unpack_stack_micro_zone(v: u8) -> (u8, u8, StackedState) {
    let position = (v >> 4) & 0b1111;
    let direction = (v >> 2) & 0b11;
    (position, direction, StackedState::from_u2(v))
}

/// Read just the `position` field under the stack layout. Returns 0-15.
pub fn micro_zone_position(v: u8) -> u8 {
    (v >> 4) & 0b1111
}

/// Read just the `direction` field under the stack layout. Returns
/// 0-3; values 0/1/2 are [`STACK_DIR_UP`] / [`STACK_DIR_DOWN`] /
/// [`STACK_DIR_HEX`]. Value 3 is reserved.
pub fn micro_zone_direction(v: u8) -> u8 {
    (v >> 2) & 0b11
}

/// Pack a `micro_zone` byte for a `Slot` (parent-pointer mode).
/// Layout matches the stack layout
/// (`[position:4 | direction:2 | state:2]`) with `position = 0` since
/// position from root is implicit (walk parent pointers via
/// `micro_location`). Direction is stored explicitly so render /
/// parent-resolve don't have to climb the chain to derive it.
pub fn pack_slot_micro_zone(direction: u8) -> u8 {
    let dir = direction & 0b11;
    (dir << 2) | StackedState::Slot.to_u2()
}

// ---- micro_location ----------------------------------------------------

pub fn pack_micro_location_xy(x: i16, y: i16) -> u32 {
    ((x as u16 as u32) << 16) | (y as u16 as u32)
}

pub fn unpack_micro_location_xy(v: u32) -> (i16, i16) {
    ((v >> 16) as u16 as i16, v as u16 as i16)
}

pub fn pack_micro_location_card_id(card_id: u32) -> u32 {
    card_id
}

pub fn unpack_micro_location_card_id(v: u32) -> u32 {
    v
}

// ---- packed_definition -------------------------------------------------
//
// u16 layout: `[ card_type: u4 | def_id: u12 ]`. The `card_category`
// dimension was retired (see
// docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md) — `category` had never
// been populated outside of the single `default = 0` value, so its
// 4-bit slot collapsed into `def_id` to give 4095 distinct defs per
// type. Subscription mask `packed_definition < 0x4000` (public types
// 0..=3) still works because the top 4 bits are still `card_type`.

/// Max `def_id` value that fits in `packed_definition`'s low 12 bits.
pub const DEF_ID_MAX: u16 = 0x0FFF;

/// Bit mask isolating the `def_id` field of a `packed_definition`.
pub const DEF_ID_MASK: u16 = 0x0FFF;

/// Bit mask isolating the `card_type` field of a `packed_definition`.
pub const CARD_TYPE_MASK: u16 = 0xF000;

pub fn pack_definition(card_type: u8, def_id: u16) -> u16 {
    (((card_type & 0xF) as u16) << 12) | (def_id & DEF_ID_MASK)
}

pub fn unpack_definition(v: u16) -> (u8, u16) {
    (((v >> 12) & 0xF) as u8, v & DEF_ID_MASK)
}

// ---- zone_definition (u8 = u4 card_type | u4 0) -----------------------
//
// Lower nibble is reserved (formerly `card_category`). Kept u8 for
// schema stability rather than narrowing the `Zone.packed_definition`
// column to u4 — the byte is on the wire either way.

pub fn pack_zone_definition(card_type: u8) -> u8 {
    (card_type & 0xF) << 4
}

pub fn unpack_zone_definition(v: u8) -> u8 {
    (v >> 4) & 0xF
}

// ---- recipe (u16 = u3 type | u3 category | u10 id) -------------------

pub const RECIPE_TYPE_OR_CATEGORY_MASK: u16 = 0x7;
pub const RECIPE_ID_MASK: u16 = 0x3FF;

pub fn pack_recipe(recipe_type: u8, recipe_category: u8, recipe_id: u16) -> u16 {
    (((recipe_type as u16) & RECIPE_TYPE_OR_CATEGORY_MASK) << 13)
        | (((recipe_category as u16) & RECIPE_TYPE_OR_CATEGORY_MASK) << 10)
        | (recipe_id & RECIPE_ID_MASK)
}

pub fn unpack_recipe(v: u16) -> (u8, u8, u16) {
    (
        ((v >> 13) & RECIPE_TYPE_OR_CATEGORY_MASK) as u8,
        ((v >> 10) & RECIPE_TYPE_OR_CATEGORY_MASK) as u8,
        v & RECIPE_ID_MASK,
    )
}

// ---- zone tile storage (16 u64 holding 64 u16 tile slots) ------------
//
// Each zone has 64 tiles (8 × 8 grid). Each tile slot is u16 wide:
//
//     [ def_id:u12 | stock0:u2 | stock1:u2 ]
//
//   - `def_id` (low 12 bits): the tile's `CardDefinition` packed_id
//     payload — same value the per-card `packed_definition` carries.
//   - `stock0` / `stock1` (bits 12-13, 14-15): u2 values for the
//     def's declared row-mutable aspect slots (see
//     `CardDefinition.stock`). The def maps slot index → aspect.
//
// 64 tiles × 16 bits = 1024 bits = exactly 16 u64. 8 tiles per row =
// 128 bits = 2 u64 per row, no boundary straddling — unlike the u12
// layout this replaces. See docs/TILE_ASPECTS.md.

/// Number of u64 fields in the zone tile-data packing.
pub const ZONE_TILE_U64_COUNT: usize = 16;

/// Number of tiles per zone (8 × 8 grid).
pub const ZONE_TILE_COUNT: usize = 64;

/// Bit width of a single tile slot (def_id + two stocks).
pub const ZONE_TILE_BITS: usize = 16;

/// Number of stock slots per tile. Matches `MAX_STOCK_SLOTS` on the
/// def side. Slot 0 lives at bits 12-13, slot 1 at bits 14-15.
pub const ZONE_TILE_STOCK_SLOTS: usize = 2;

/// Max value a stock slot can store (u2).
pub const ZONE_TILE_STOCK_MAX: u8 = 0x3;

/// Bit mask isolating one tile's u16 within its u64 (after shifting
/// to the tile's bit offset).
const TILE_MASK: u64 = 0xFFFF;

/// Read tile `idx` (0..64) — returns `(def_id, stock0, stock1)`.
pub fn tile_full(packed: &[u64; ZONE_TILE_U64_COUNT], idx: usize) -> (u16, u8, u8) {
    debug_assert!(idx < ZONE_TILE_COUNT, "tile index {} out of range", idx);
    let u64_idx = idx / 4; // 4 tiles per u64
    let bit_offset = (idx % 4) * 16;
    let slot = (packed[u64_idx] >> bit_offset) & TILE_MASK;
    let def_id = (slot & 0x0FFF) as u16;
    let stock0 = ((slot >> 12) & 0x3) as u8;
    let stock1 = ((slot >> 14) & 0x3) as u8;
    (def_id, stock0, stock1)
}

/// Read just the def_id (low 12 bits) of tile `idx`.
pub fn tile_def_id(packed: &[u64; ZONE_TILE_U64_COUNT], idx: usize) -> u16 {
    debug_assert!(idx < ZONE_TILE_COUNT, "tile index {} out of range", idx);
    let u64_idx = idx / 4;
    let bit_offset = (idx % 4) * 16;
    ((packed[u64_idx] >> bit_offset) & 0x0FFF) as u16
}

/// Read tile `idx`'s stock slot `slot` (0 or 1). Returns 0..=3.
pub fn tile_stock(packed: &[u64; ZONE_TILE_U64_COUNT], idx: usize, slot: usize) -> u8 {
    debug_assert!(idx < ZONE_TILE_COUNT, "tile index {} out of range", idx);
    debug_assert!(slot < ZONE_TILE_STOCK_SLOTS, "stock slot {} out of range", slot);
    let u64_idx = idx / 4;
    let bit_offset = (idx % 4) * 16 + 12 + (slot * 2);
    ((packed[u64_idx] >> bit_offset) & 0x3) as u8
}

/// Write tile `idx`'s full u16 slot. Each field is masked to its
/// declared width — excess bits are silently dropped, not panicked
/// on.
pub fn set_tile_full(
    packed: &mut [u64; ZONE_TILE_U64_COUNT],
    idx: usize,
    def_id: u16,
    stock0: u8,
    stock1: u8,
) {
    debug_assert!(idx < ZONE_TILE_COUNT, "tile index {} out of range", idx);
    let u64_idx = idx / 4;
    let bit_offset = (idx % 4) * 16;
    let mask = TILE_MASK << bit_offset;
    let value = (def_id as u64 & 0x0FFF)
        | ((stock0 as u64 & 0x3) << 12)
        | ((stock1 as u64 & 0x3) << 14);
    packed[u64_idx] = (packed[u64_idx] & !mask) | (value << bit_offset);
}

/// Write a single stock slot on tile `idx`. Other bits in the u16
/// slot (the def_id and the other stock) are left untouched.
pub fn set_tile_stock(
    packed: &mut [u64; ZONE_TILE_U64_COUNT],
    idx: usize,
    slot: usize,
    value: u8,
) {
    debug_assert!(idx < ZONE_TILE_COUNT, "tile index {} out of range", idx);
    debug_assert!(slot < ZONE_TILE_STOCK_SLOTS, "stock slot {} out of range", slot);
    let u64_idx = idx / 4;
    let bit_offset = (idx % 4) * 16 + 12 + (slot * 2);
    let mask = 0x3u64 << bit_offset;
    let v = (value as u64 & 0x3) << bit_offset;
    packed[u64_idx] = (packed[u64_idx] & !mask) | v;
}

/// Decode one row of 8 tiles. Returns `(def_id, stock0, stock1)` per
/// column, row-major (row 0 = tile indices 0..=7, row 1 = 8..=15,
/// etc.).
pub fn tile_row(packed: &[u64; ZONE_TILE_U64_COUNT], row: usize) -> [(u16, u8, u8); 8] {
    let mut out = [(0u16, 0u8, 0u8); 8];
    let base = row * 8;
    for col in 0..8 {
        out[col] = tile_full(packed, base + col);
    }
    out
}

pub fn set_tile_row(
    packed: &mut [u64; ZONE_TILE_U64_COUNT],
    row: usize,
    slots: &[(u16, u8, u8); 8],
) {
    debug_assert!(row < 8, "row {} out of range", row);
    let base = row * 8;
    for (col, (def_id, stock0, stock1)) in slots.iter().enumerate() {
        set_tile_full(packed, base + col, *def_id, *stock0, *stock1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_at_roundtrip() {
        let v = pack_valid_at(0x0000_DEAD_BEEF_1234, 0x5678);
        assert_eq!(valid_at_time(v), 0x0000_DEAD_BEEF_1234);
        assert_eq!(v & 0xFFFF, 0x5678);
    }

    #[test]
    fn macro_zone_signed_roundtrip() {
        let v = pack_macro_zone(-1, 1);
        assert_eq!(unpack_macro_zone(v), (-1, 1));
        let v = pack_macro_zone(i16::MIN, i16::MAX);
        assert_eq!(unpack_macro_zone(v), (i16::MIN, i16::MAX));
    }

    #[test]
    fn micro_zone_roundtrip() {
        let v = pack_micro_zone(5, 3, StackedState::Free);
        assert_eq!(unpack_micro_zone(v), (5, 3, StackedState::Free));
    }

    #[test]
    fn stack_micro_zone_roundtrip() {
        for dir in [STACK_DIR_UP, STACK_DIR_DOWN, STACK_DIR_HEX] {
            let v = pack_stack_micro_zone(7, dir, StackedState::OnRoot);
            assert_eq!(unpack_stack_micro_zone(v), (7, dir, StackedState::OnRoot));
            let v = pack_stack_micro_zone(15, dir, StackedState::OnRoot);
            assert_eq!(unpack_stack_micro_zone(v), (15, dir, StackedState::OnRoot));
        }
        // position saturates at 4 bits.
        let v = pack_stack_micro_zone(0xFF, STACK_DIR_UP, StackedState::OnRoot);
        assert_eq!(micro_zone_position(v), 15);
        assert_eq!(micro_zone_direction(v), STACK_DIR_UP);
        assert_eq!(micro_zone_state(v), StackedState::OnRoot);
    }

    #[test]
    fn stack_layout_gate() {
        assert!(is_stack_layout(StackedState::OnRoot, 1));
        assert!(!is_stack_layout(StackedState::Free, 1));
        // `Slot` rows are server-authoritative — even though their byte
        // layout matches the stack layout, the mirror's preserve gate
        // doesn't fire for them, so `is_stack_layout` returns false.
        assert!(!is_stack_layout(StackedState::Slot, 1));
        // World surfaces (>= 64) keep legacy layout regardless of state.
        assert!(!is_stack_layout(StackedState::OnRoot, 64));
        assert!(!is_stack_layout(StackedState::OnRoot, 200));
    }

    #[test]
    fn slot_micro_zone_roundtrip() {
        for dir in [STACK_DIR_UP, STACK_DIR_DOWN, STACK_DIR_HEX] {
            let v = pack_slot_micro_zone(dir);
            assert_eq!(micro_zone_state(v), StackedState::Slot);
            assert_eq!(micro_zone_direction(v), dir);
            assert_eq!(micro_zone_position(v), 0);
        }
    }

    #[test]
    #[should_panic(expected = "reserved (legacy OnHex)")]
    fn legacy_onhex_value_panics_in_from_u2() {
        let _ = StackedState::from_u2(3);
    }

    #[test]
    fn micro_location_xy_signed() {
        let v = pack_micro_location_xy(-100, 200);
        assert_eq!(unpack_micro_location_xy(v), (-100, 200));
    }

    #[test]
    fn definition_roundtrip() {
        // type=0xA, def_id=0xABC.
        let v = pack_definition(0xA, 0xABC);
        assert_eq!(unpack_definition(v), (0xA, 0xABC));
        // Saturate def_id at the u12 max.
        let v = pack_definition(0x7, 0xFFF);
        assert_eq!(unpack_definition(v), (0x7, 0xFFF));
        // Excess bits in def_id are masked off, not panicked on.
        let v = pack_definition(0x3, 0x1234);
        assert_eq!(unpack_definition(v), (0x3, 0x234));
        // Public-type subscription mask: type < 4 ⇔ packed < 0x4000.
        assert!(pack_definition(0x3, 0xFFF) < 0x4000);
        assert!(pack_definition(0x4, 0x000) >= 0x4000);
    }

    #[test]
    fn recipe_roundtrip() {
        let v = pack_recipe(0b101, 0b011, 0x2F4);
        assert_eq!(unpack_recipe(v), (0b101, 0b011, 0x2F4));
        // Saturate each field at its mask.
        let v = pack_recipe(0x7, 0x7, RECIPE_ID_MASK);
        assert_eq!(unpack_recipe(v), (0x7, 0x7, RECIPE_ID_MASK));
        // Overflow bits in inputs should be masked off cleanly.
        let v = pack_recipe(0xFF, 0xFF, 0xFFFF);
        assert_eq!(unpack_recipe(v), (0x7, 0x7, RECIPE_ID_MASK));
    }

    #[test]
    fn zone_definition_roundtrip() {
        let v = pack_zone_definition(0xC);
        assert_eq!(unpack_zone_definition(v), 0xC);
        // Lower nibble unused after the category retire — always 0.
        assert_eq!(v & 0xF, 0x0);
    }

    #[test]
    fn tile_full_roundtrip() {
        let mut packed = [0u64; ZONE_TILE_U64_COUNT];
        // Read-empty: every slot zero.
        for i in 0..ZONE_TILE_COUNT {
            assert_eq!(tile_full(&packed, i), (0, 0, 0));
        }
        // Write each tile to a distinct (def_id, stock0, stock1) and
        // read it back. Defs use the full u12; stocks cycle through
        // 0..=3 so we exercise all u2 values.
        for i in 0..ZONE_TILE_COUNT {
            set_tile_full(
                &mut packed,
                i,
                (i + 1) as u16,
                (i % 4) as u8,
                ((i + 1) % 4) as u8,
            );
        }
        for i in 0..ZONE_TILE_COUNT {
            assert_eq!(
                tile_full(&packed, i),
                ((i + 1) as u16, (i % 4) as u8, ((i + 1) % 4) as u8),
            );
        }
    }

    #[test]
    fn tile_field_masking() {
        // Each field is masked to its declared width; excess bits
        // are silently dropped, not panicked on. Confirms the high
        // bits don't bleed into neighbour fields.
        let mut packed = [0u64; ZONE_TILE_U64_COUNT];
        // def_id u12 — 0xFFFF gets masked to 0xFFF.
        set_tile_full(&mut packed, 0, 0xFFFF, 0, 0);
        assert_eq!(tile_full(&packed, 0), (0xFFF, 0, 0));
        // stock u2 — 0xFF gets masked to 0x3.
        set_tile_full(&mut packed, 0, 0, 0xFF, 0xFF);
        assert_eq!(tile_full(&packed, 0), (0, 3, 3));
        // Neighbours stay zero across all the masking.
        assert_eq!(tile_full(&packed, 1), (0, 0, 0));
    }

    #[test]
    fn tile_stock_isolated_writes() {
        // `set_tile_stock` mutates one stock without touching the
        // def or the other stock.
        let mut packed = [0u64; ZONE_TILE_U64_COUNT];
        set_tile_full(&mut packed, 7, 0xABC, 1, 2);
        set_tile_stock(&mut packed, 7, 0, 3);
        assert_eq!(tile_full(&packed, 7), (0xABC, 3, 2));
        set_tile_stock(&mut packed, 7, 1, 0);
        assert_eq!(tile_full(&packed, 7), (0xABC, 3, 0));
        // tile_stock reads the same value
        assert_eq!(tile_stock(&packed, 7, 0), 3);
        assert_eq!(tile_stock(&packed, 7, 1), 0);
    }

    #[test]
    fn tile_neighbour_independence() {
        // 4 tiles share a u64. Writing one tile must not corrupt
        // the other three. Pin tile 5's bits in the middle of u64[1]
        // (which holds tiles 4..=7) and confirm the surrounding
        // tiles stay zero, then mutate tile 5's stock and confirm
        // 4 / 6 / 7 still report zero.
        let mut packed = [0u64; ZONE_TILE_U64_COUNT];
        set_tile_full(&mut packed, 5, 0xABC, 2, 1);
        for &idx in &[0, 4, 6, 7, 8, 63] {
            assert_eq!(tile_full(&packed, idx), (0, 0, 0));
        }
        set_tile_stock(&mut packed, 5, 0, 0);
        for &idx in &[0, 4, 6, 7, 8, 63] {
            assert_eq!(tile_full(&packed, idx), (0, 0, 0));
        }
        // tile 5 retained its def + slot 1 stock.
        assert_eq!(tile_full(&packed, 5), (0xABC, 0, 1));
    }

    #[test]
    fn tile_row_decode() {
        let mut packed = [0u64; ZONE_TILE_U64_COUNT];
        for col in 0..8 {
            set_tile_full(&mut packed, col, 100 + col as u16, 1, 2);
        }
        let row0 = tile_row(&packed, 0);
        for (col, entry) in row0.iter().enumerate() {
            assert_eq!(*entry, (100 + col as u16, 1, 2));
        }
        // Row 1 is still empty.
        let row1 = tile_row(&packed, 1);
        assert_eq!(row1, [(0, 0, 0); 8]);
    }
}
