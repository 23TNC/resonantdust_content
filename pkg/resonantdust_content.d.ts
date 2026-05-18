/* tslint:disable */
/* eslint-disable */

/**
 * Every registered texture definition, in stable-id order. Each entry
 * carries `id`, `cardType`, `aspectId`, `aspectName`, `object`,
 * `size`, and `scale: { min, max }`. Returns an empty array when no
 * textures are registered. Throws on registry-build failure.
 *
 * Called once at startup by `TextureRegistry.ts` to build the client-side
 * lookup map; not intended for per-frame use.
 */
export function allTextures(): any;

/**
 * Look up an aspect by id. Returns the `Aspect` object (with `id`,
 * `name`, `description`, `icon`, `group` fields) or `null` for
 * `ASPECT_NONE` (id 0) and unknown ids. Throws on registry-build
 * failure.
 */
export function aspectInfo(id: number): any;

/**
 * Bit position (0..=7) of a card-flag by name (e.g. `"drop_hold"`,
 * `"position_locked"`, `"dead"`). Returns `undefined` if no flag with
 * that name is declared in `cards/flags.json`. Throws on registry-build
 * failure. JS-side callers typically convert to a mask via
 * `1 << bit` before testing against `row.flags`.
 */
export function cardFlagBit(name: string): number | undefined;

/**
 * Read the value of a multi-bit card-flag field (e.g.
 * `"progress_style"`, `"position_hold_count"`) out of a `flags`
 * u32. Returns `undefined` if no field with that name is declared in
 * `cards/flags.json`; returns the extracted unsigned value
 * otherwise. Throws on registry-build failure.
 *
 * Equivalent to `(flags >> field.shift) & field.mask`. JS-side
 * callers checking "is the count > 0?" use `value > 0`; callers
 * reading specific enum-style values (`progress_style == 1`)
 * compare directly.
 */
export function cardFlagFieldValue(flags: number, name: string): number | undefined;

/**
 * Look up the display label for a packed definition in the given
 * language, e.g. `cardLabel(packed, "en")` → `"Log"`. Falls back to
 * English when `lang` has no entry. Returns `undefined` for unknown
 * packed ids or locale entries with no label. Throws on registry-build
 * failure. Callers should fall back to `def.key` on `undefined`.
 */
export function cardLabel(packed_def: number, lang: string): string | undefined;

/**
 * Look up a `card_type` id by name (e.g. `"mini_zone"`, `"soul"`,
 * `"tile"`). Returns `undefined` for unknown names. Source of truth
 * is `content/cards/types.json`. Used by JS-side code that needs to
 * branch on a card's type (without hard-coding the numeric id).
 */
export function cardTypeId(name: string): number | undefined;

/**
 * Decode a packed `(cardType:u4 | definitionId:u12)` value into a
 * `CardDefinition`-shaped JS object. Returns `null` if no card
 * matches the packed value. Throws a string error if the card
 * registry failed to build (malformed JSON, unknown aspects, etc.).
 */
export function decodeDefinition(packed: number): any;

/**
 * Look up a card's packed value by its bare key (e.g. `"fatigue"`).
 * Returns `undefined` if no card has that key. Throws on registry-build
 * failure.
 */
export function findPackedByKey(key: string): number | undefined;

/**
 * Look up a recipe by its tree-key (third-level key under
 * `<type>/<category>/<key>` in `recipes/data/*.json`). Returns a
 * `RecipeBrief`-shaped JS object on hit, `null` on miss. Throws on
 * registry-build failure.
 *
 * Used by [`MagneticResolutionManager`](../../../../pixijs/src/game/magnetic/MagneticResolutionManager.ts)
 * to resolve a card def's `magneticRecipeKey` into the packed
 * recipe id needed for `proposeAction`, plus enough metadata
 * (slot count, direction) to drive client-side slot scanning.
 */
export function findRecipeByKey(key: string): any;

export function inventoryLayer(): number;

/**
 * Whether the given `cardType` id resolves to a hex-shaped type
 * (`"hex"` in `cards/types.json`). Throws on registry-build failure.
 */
export function isHexType(type_id: number): boolean;

export function isStackLayout(stacked_state: number, surface: number): boolean;

/**
 * Try to match a magnetic recipe against `(root_def, slot_defs)`.
 * Mirrors the server-side `match_magnetic_recipe` (Phase 2 of the
 * magnetic rewrite). Returns a `StackMatch`-shaped JS object on
 * success or `null` if the predicates don't fit. Throws on
 * registry-build failure or invalid direction.
 *
 * `direction` is `0 = up`, `1 = down`. The client looks up the
 * magnetic card's `magneticRecipeKey` to know the direction (the
 * recipe's `recipe_type` encodes it).
 */
export function matchMagneticRecipe(root_def: number, slot_defs: Uint16Array, direction: number, root_above: Uint16Array, actor_above: Uint16Array, root_below: Uint16Array, actor_below: Uint16Array): any;

/**
 * Find the best-matching `Stack(direction)` recipe for a chain.
 * `hex_def` is the packed definition of the hex card the chain root is
 * attached to (`0` if not stacked on hex). `root_def` is the loose
 * root's packed definition. `slot_defs` are the packed definitions of
 * cards stacked above (`direction = 0` / "up") or below
 * (`direction = 1` / "down") the root, in chain order.
 *
 * `root_above` / `actor_above` / `root_below` / `actor_below` are the
 * packed definitions of cards stacked on each role's soul card in
 * each direction (UP = equipment / above the soul, DOWN = action
 * stack / below the soul). They feed the `has` / `reagents.has` /
 * `has_below` / `reagents.has_below` feasibility filter: recipes
 * whose has-predicates can't find any matching card in the
 * corresponding pool are skipped before scoring. Pass empty arrays
 * to mean "no equipment / nothing on the soul stack" — recipes
 * that declare has-predicates will then be filtered out, which is
 * the correct behaviour for an unattached player.
 *
 * Unknown packed defs in any pool array are silently skipped (treat
 * the registry as authoritative — a wire-side glitch shouldn't
 * crash matching).
 *
 * Returns a `StackMatch` object on success (with `recipeIndex`,
 * `slotStart`, `slotCount`, `hasRoot`, `hasHex`) or `null` if no
 * recipe matched. Throws on registry-build failure or invalid
 * direction.
 */
export function matchStackRecipe(hex_def: number, hex_stock0: number, hex_stock1: number, hex_has_stocks: number, root_def: number, slot_defs: Uint16Array, direction: number, root_above: Uint16Array, actor_above: Uint16Array, root_below: Uint16Array, actor_below: Uint16Array): any;

export function miniZoneLayer(): number;

export function packDefinition(card_type: number, def_id: number): number;

export function packMacroZone(q: number, r: number): number;

export function packMicroZone(q: number, r: number, stacked_state: number): number;

export function packSlotMicroZone(direction: number): number;

export function packStackMicroZone(position: number, direction: number, stacked_state: number): number;

export function packValidAt(time_ms: bigint, sequence: number): bigint;

export function packZoneDefinition(card_type: number): number;

export function pocketDimensionLayer(): number;

/**
 * All starter packs registered for a given soul card key (e.g.
 * `"human"`). Returns an array of `StarterPack` objects (`id`,
 * `soul`, `packId`, `contents: [{cardKey, packedDefinition,
 * count}]`). Empty array for unknown soul keys. Throws on
 * registry-build failure.
 *
 * Used by the character-create panel to enumerate which packs the
 * player can pick from. JS-side filtering by soul is unnecessary
 * since this is already soul-scoped at the call site.
 */
export function starterPacksForSoul(soul: string): any;

/**
 * Read the numeric value of a named trait off a packed card
 * definition. Returns `null` when:
 * - the trait name isn't in `traits.json`,
 * - the def doesn't carry that trait,
 * - or the packed def doesn't resolve to a registered card.
 *
 * Source-of-truth pair with the server's
 * `def.trait_value(trait_id("name"))` path — both go through the
 * same `CardDefinition::trait_value` lookup, so client and server
 * agree on cost / speed numbers by construction.
 *
 * Used by client A* (`pixijs/src/game/world/pathfind.ts`) to
 * resolve per-tile `cost` and per-soul `speed` for the step-time
 * calculation, mirroring the server validator in
 * `movement::move_soul_path`.
 */
export function traitValue(packed_def: number, name: string): number | undefined;

export function unpackDefinition(v: number): any;

export function unpackMacroZone(v: number): any;

export function unpackMicroZone(v: number): any;

export function unpackStackMicroZone(v: number): any;

export function unpackZoneDefinition(v: number): number;

export function validAtTime(packed: bigint): bigint;

export function worldLayer(): number;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly allTextures: () => [number, number, number];
    readonly aspectInfo: (a: number) => [number, number, number];
    readonly cardFlagBit: (a: number, b: number) => [number, number, number];
    readonly cardFlagFieldValue: (a: number, b: number, c: number) => [number, number, number];
    readonly cardLabel: (a: number, b: number, c: number) => [number, number, number, number];
    readonly cardTypeId: (a: number, b: number) => [number, number, number];
    readonly decodeDefinition: (a: number) => [number, number, number];
    readonly findPackedByKey: (a: number, b: number) => [number, number, number];
    readonly findRecipeByKey: (a: number, b: number) => [number, number, number];
    readonly inventoryLayer: () => number;
    readonly isHexType: (a: number) => [number, number, number];
    readonly isStackLayout: (a: number, b: number) => number;
    readonly matchMagneticRecipe: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number) => [number, number, number];
    readonly matchStackRecipe: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number) => [number, number, number];
    readonly miniZoneLayer: () => number;
    readonly packDefinition: (a: number, b: number) => number;
    readonly packMacroZone: (a: number, b: number) => number;
    readonly packMicroZone: (a: number, b: number, c: number) => number;
    readonly packSlotMicroZone: (a: number) => number;
    readonly packStackMicroZone: (a: number, b: number, c: number) => number;
    readonly packValidAt: (a: bigint, b: number) => bigint;
    readonly packZoneDefinition: (a: number) => number;
    readonly pocketDimensionLayer: () => number;
    readonly starterPacksForSoul: (a: number, b: number) => [number, number, number];
    readonly traitValue: (a: number, b: number, c: number) => [number, number, number];
    readonly unpackDefinition: (a: number) => [number, number, number];
    readonly unpackMacroZone: (a: number) => [number, number, number];
    readonly unpackMicroZone: (a: number) => [number, number, number];
    readonly unpackStackMicroZone: (a: number) => [number, number, number];
    readonly unpackZoneDefinition: (a: number) => number;
    readonly validAtTime: (a: bigint) => bigint;
    readonly worldLayer: () => number;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
