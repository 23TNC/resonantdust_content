/* tslint:disable */
/* eslint-disable */

/**
 * Bit position (0..=7) of a card-flag by name (e.g. `"drop_hold"`,
 * `"position_locked"`, `"dead"`). Returns `undefined` if no flag with
 * that name is declared in `cards/flags.json`. Throws on registry-build
 * failure. JS-side callers typically convert to a mask via
 * `1 << bit` before testing against `row.flags`.
 */
export function cardFlagBit(name: string): number | undefined;

/**
 * Decode a packed `(cardType:u4 | cardCategory:u4 | definitionId:u8)` value
 * into a `CardDefinition`-shaped JS object. Returns `null` if no card
 * matches the packed value. Throws a string error if the card registry
 * failed to build (malformed JSON, unknown aspects, etc.).
 */
export function decodeDefinition(packed: number): any;

/**
 * Look up a card's packed value by its bare key (e.g. `"fatigue"`).
 * Returns `undefined` if no card has that key. Throws on registry-build
 * failure.
 */
export function findPackedByKey(key: string): number | undefined;

/**
 * Whether the given `cardType` id resolves to a hex-shaped type
 * (`"hex"` in `cards/types.json`). Throws on registry-build failure.
 */
export function isHexType(type_id: number): boolean;

/**
 * Find the best-matching `Stack(direction)` recipe for a chain.
 * `hex_def` is the packed definition of the hex card the chain root is
 * attached to (`0` if not stacked on hex). `root_def` is the loose
 * root's packed definition. `slot_defs` are the packed definitions of
 * cards stacked above (`direction = 0` / "up") or below
 * (`direction = 1` / "down") the root, in chain order.
 *
 * Returns a `StackMatch` object on success (with `recipeIndex`,
 * `slotStart`, `slotCount`, `hasRoot`, `hasHex`) or `null` if no
 * recipe matched. Throws on registry-build failure or invalid
 * direction. The `slotStart` / `slotCount` fields tell the caller
 * which slice of the chain (`chain = [root] ++ slot_defs`) fills the
 * recipe's slot list — needed to assemble the `propose_action`
 * reducer call correctly.
 */
export function matchStackRecipe(hex_def: number, root_def: number, slot_defs: Uint16Array, direction: number): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly cardFlagBit: (a: number, b: number) => [number, number, number];
    readonly decodeDefinition: (a: number) => [number, number, number];
    readonly findPackedByKey: (a: number, b: number) => [number, number, number];
    readonly isHexType: (a: number) => [number, number, number];
    readonly matchStackRecipe: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
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
