/* tslint:disable */
/* eslint-disable */

/**
 * Every registered soul-scope blueprint in stable-id order.
 * Called by the wrench panel to enumerate the catalog for display.
 */
export function allBlueprints(): any;

/**
 * Player-scope analog of [`all_blueprints`]. Called by the dna
 * (🧬) panel to enumerate the player-blueprint catalog.
 */
export function allPlayerBlueprints(): any;

/**
 * Every registered texture definition, in stable-id order. Each entry
 * carries `id`, `aspectId`, `aspectName`, `size`,
 * `scale: { min, max }`, and `anchor: { x, y }`. Returns an empty
 * array when no aspect carries render metadata. Throws on
 * registry-build failure.
 *
 * Post card-object unification (see
 * docs/CARD_OBJECT_UNIFICATION.md) entries are aspect-keyed and the
 * pack-folder on disk is named `<size>_<aspectName>_pack/` — pack
 * name and aspect name are the same string.
 *
 * Called once at startup by `TextureRegistry.ts` to build the
 * client-side lookup map; not intended for per-frame use.
 */
export function allTextures(): any;

/**
 * Look up an aspect's numeric id by its declared name (the JSON
 * key under `cards/aspects.json` — `"wood"`, `"corpus+"`, etc.).
 * Returns `undefined` when the name isn't registered. Throws on
 * registry-build failure.
 *
 * Used by the client recipe matcher to evaluate
 * `<path>.aspect.<name>.min: <N>` predicates: the name appears in
 * the recipe segments, but card defs store aspect entries keyed by
 * numeric id — this helper bridges the two.
 */
export function aspectIdByName(name: string): number | undefined;

/**
 * Look up an aspect by id. Returns the `Aspect` object (with `id`,
 * `name`, `description`, `icon`, `group` fields) or `null` for
 * `ASPECT_NONE` (id 0) and unknown ids. Throws on registry-build
 * failure.
 */
export function aspectInfo(id: number): any;

/**
 * Read the numeric value of a named aspect off a packed card
 * definition. Returns `null` when:
 * - the aspect name isn't in `aspects.json`,
 * - the def doesn't carry that aspect,
 * - or the packed def doesn't resolve to a registered card.
 *
 * Source-of-truth pair with the server's
 * `def.aspect_value(aspect_id("name"))` path — both go through the
 * same `CardDefinition::aspect_value` lookup, so client and server
 * agree on cost / speed / inventory / etc. numbers by construction.
 *
 * Used by client A* (`pixijs/src/game/world/pathfind.ts`) to
 * resolve per-tile `cost` and per-soul `speed` for the step-time
 * calculation, mirroring the server validator in
 * `movement::move_soul_path`.
 */
export function aspectValue(packed_def: number, name: string): number | undefined;

/**
 * Look up a soul-scope blueprint by its stable `u16` id. Returns
 * the full Blueprint object or `null` if the id isn't registered.
 * Throws on registry-build failure.
 */
export function blueprintById(id: number): any;

/**
 * Look up a soul-scope blueprint by its source-key. Returns the
 * full Blueprint object or `null`. Throws on registry-build failure.
 */
export function blueprintByKey(key: string): any;

/**
 * **Legacy** — bit position (0..=31) of a card-flag by name, searched
 * across both `cards_state` and `cards_bk` fields (state first).
 * Returns `undefined` if no single-bit flag with that name exists in
 * either field. Ambiguous against the split-field schema — callers
 * that need to know which host integer the bit lives in should use
 * [`cardFlagBitIn`] with an explicit field name instead.
 */
export function cardFlagBit(name: string): number | undefined;

/**
 * Bit position (0..=31) of a single-bit flag in a specific field.
 * `field` is `"cards_state"` or `"cards_bk"`. Returns `undefined` if
 * no single-bit flag with that name is declared in the given field.
 * Preferred over [`cardFlagBit`] for new call sites — explicit field
 * argument means lookups can't accidentally collide across fields.
 */
export function cardFlagBitIn(field: string, name: string): number | undefined;

/**
 * `(shift, width)` of a multi-bit flag field in a specific field.
 * Returns `undefined` if no multi-bit field with that name is
 * declared in the given field. Use the returned pair to mask:
 * `mask = ((1 << width) - 1) << shift`, value extract:
 * `(host >> shift) & ((1 << width) - 1)`.
 */
export function cardFlagFieldShape(field: string, name: string): Uint8Array | undefined;

/**
 * **Legacy** — read the value of a multi-bit card-flag field by
 * name, searching across both `cards_state` and `cards_bk` (state
 * first). Caller passes a single `flags` u32 that should be the
 * matching host integer; ambiguous against the split-field schema.
 * Prefer [`cardFlagFieldValueIn`] with an explicit field name for
 * new call sites.
 */
export function cardFlagFieldValue(flags: number, name: string): number | undefined;

/**
 * Field-routing helper for multi-bit fields — given both host
 * integers and a field name, returns the extracted value from
 * whichever field declares it (state-first lookup). Returns
 * `undefined` for unknown field names.
 */
export function cardFlagFieldValueAny(state: number, bk: number, name: string): number | undefined;

/**
 * Read the value of a multi-bit field in a specific host integer.
 * `field` is `"cards_state"` or `"cards_bk"`; `host` is the value
 * of the corresponding `Card.flags_state` / `Card.flags_bk` column.
 * Returns `undefined` if no multi-bit field with that name is
 * declared in the given field.
 */
export function cardFlagFieldValueIn(field: string, host: number, name: string): number | undefined;

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
 * Field-routing helper — given **both** flag host integers and a
 * flag name, returns `true` if the named single-bit flag is set in
 * whichever field declares it. Looks up `cards_state` first then
 * `cards_bk`; consults only the matching host. Callers pass the
 * whole `(state, bk)` pair from the card row so the lookup is
 * unambiguous against the split schema.
 *
 * Returns `false` for unknown flag names (the safe default for
 * "absent") and for cards whose bit is clear in the matching host.
 */
export function hasCardFlag(state: number, bk: number, name: string): boolean;

export function inventoryLayer(): number;

/**
 * Whether the given `cardType` id resolves to a hex-shaped type
 * (`"hex"` in `cards/types.json`). Throws on registry-build failure.
 */
export function isHexType(type_id: number): boolean;

export function isStackLayout(stacked_state: number, surface: number): boolean;

export function miniZoneLayer(): number;

export function packDefinition(card_type: number, def_id: number): number;

export function packMacroZone(q: number, r: number): number;

export function packMicroZone(q: number, r: number, stacked_state: number): number;

export function packSlotMicroZone(direction: number): number;

export function packStackMicroZone(position: number, direction: number, stacked_state: number): number;

export function packValidAt(time_ms: bigint, sequence: number): bigint;

export function packZoneDefinition(card_type: number): number;

/**
 * Player-scope analog of [`blueprint_by_id`].
 */
export function playerBlueprintById(id: number): any;

/**
 * Player-scope analog of [`blueprint_by_key`].
 */
export function playerBlueprintByKey(key: string): any;

export function pocketDimensionLayer(): number;

/**
 * Look up a recipe by its stable `u16` id (the value
 * `proposeAction` takes as `recipeId`). Returns the full Recipe IR
 * serialized to JS — `{ id, input[], output[], iterators[], anchors }`
 * — or `null` if the id isn't registered. Throws on registry-build
 * failure.
 *
 * Used by callers that already have the id (e.g., looking up a
 * magnetic recipe via a card def's `magnetic.recipe` key resolved
 * through `findPackedByKey`-style indirection).
 */
export function recipeById(id: number): any;

/**
 * Look up a recipe by its source-key (e.g. `"cut_tree"`,
 * `"strike_success"`). Returns the full Recipe IR serialized to JS
 * or `null` if no recipe with that key is registered. Throws on
 * registry-build failure.
 *
 * Used by callers that have a string key in hand — for example, a
 * card def's `magnetic.recipe` field, or recipe-name lookups in
 * debug tooling.
 */
export function recipeByKey(key: string): any;

/**
 * Every registered recipe in priority-tiered order (highest priority
 * first). Each entry is `{ id: u16, recipe: Recipe }` — the `id` is
 * the stable u16 from `recipes/id.json` (what `proposeAction` takes),
 * the `recipe` is the parsed IR including `iterators` and `anchors`.
 *
 * Priority order is determined by [`crate::recipe_core::AnchorSet`]
 * — anchor count first, then anchor priority (hex > root > up > down).
 * The client matcher walks this array in order and stops at the first
 * tier that yields successful binding(s).
 *
 * Returns an empty array when no recipes are registered. Throws on
 * registry-build failure.
 */
export function recipesAll(): any;

/**
 * Stable blueprint ids granted to a player on creating a character
 * of the given soul. Sourced from the soul's `"blueprints"` array in
 * `starter_packs/data/*.json`. Returns an empty array for souls that
 * don't declare any. Throws on registry-build failure.
 *
 * Each id resolves to a full `Blueprint` via `blueprintById`.
 */
export function starterBlueprintsForSoul(soul: string): Uint16Array;

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
    readonly allBlueprints: () => [number, number, number];
    readonly allPlayerBlueprints: () => [number, number, number];
    readonly allTextures: () => [number, number, number];
    readonly aspectIdByName: (a: number, b: number) => [number, number, number];
    readonly aspectInfo: (a: number) => [number, number, number];
    readonly aspectValue: (a: number, b: number, c: number) => [number, number, number];
    readonly blueprintById: (a: number) => [number, number, number];
    readonly blueprintByKey: (a: number, b: number) => [number, number, number];
    readonly cardFlagBit: (a: number, b: number) => [number, number, number];
    readonly cardFlagBitIn: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly cardFlagFieldShape: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly cardFlagFieldValue: (a: number, b: number, c: number) => [number, number, number];
    readonly cardFlagFieldValueAny: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly cardFlagFieldValueIn: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly cardLabel: (a: number, b: number, c: number) => [number, number, number, number];
    readonly cardTypeId: (a: number, b: number) => [number, number, number];
    readonly decodeDefinition: (a: number) => [number, number, number];
    readonly findPackedByKey: (a: number, b: number) => [number, number, number];
    readonly hasCardFlag: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly inventoryLayer: () => number;
    readonly isHexType: (a: number) => [number, number, number];
    readonly isStackLayout: (a: number, b: number) => number;
    readonly miniZoneLayer: () => number;
    readonly packDefinition: (a: number, b: number) => number;
    readonly packMacroZone: (a: number, b: number) => number;
    readonly packMicroZone: (a: number, b: number, c: number) => number;
    readonly packSlotMicroZone: (a: number) => number;
    readonly packStackMicroZone: (a: number, b: number, c: number) => number;
    readonly packValidAt: (a: bigint, b: number) => bigint;
    readonly packZoneDefinition: (a: number) => number;
    readonly playerBlueprintById: (a: number) => [number, number, number];
    readonly playerBlueprintByKey: (a: number, b: number) => [number, number, number];
    readonly pocketDimensionLayer: () => number;
    readonly recipeById: (a: number) => [number, number, number];
    readonly recipeByKey: (a: number, b: number) => [number, number, number];
    readonly recipesAll: () => [number, number, number];
    readonly starterBlueprintsForSoul: (a: number, b: number) => [number, number, number, number];
    readonly starterPacksForSoul: (a: number, b: number) => [number, number, number];
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
