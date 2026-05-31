/* @ts-self-types="./resonantdust_content.d.ts" */

/**
 * Every registered blueprint in stable-id order. Called by the
 * wrench panel to enumerate the catalog for display.
 * @returns {any}
 */
export function allBlueprints() {
    const ret = wasm.allBlueprints();
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

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
 * @returns {any}
 */
export function allTextures() {
    const ret = wasm.allTextures();
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Look up an aspect description `variant` (dotted —
 * `"description.simple"`) by id in `lang`, falling back to English.
 * Replicates the inheritance `aspects.json` used to bake in: if the
 * aspect declares no text for `variant`, the parent chain is walked
 * and the nearest ancestor that does is used — so `berry` resolves
 * `food`'s blurb and `fuel` resolves `fire`'s without the locale
 * having to duplicate them. Returns `undefined` when neither the
 * aspect nor any ancestor declares the variant. Walk depth is bounded
 * (16) defensively, matching `is_aspect_descendant`. Throws on
 * registry-build failure.
 * @param {number} id
 * @param {string} lang
 * @param {string} variant
 * @returns {string | undefined}
 */
export function aspectDescription(id, lang, variant) {
    const ptr0 = passStringToWasm0(lang, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(variant, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.aspectDescription(id, ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    let v3;
    if (ret[0] !== 0) {
        v3 = getStringFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v3;
}

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
 * @param {string} name
 * @returns {number | undefined}
 */
export function aspectIdByName(name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.aspectIdByName(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === 0xFFFFFF ? undefined : ret[0];
}

/**
 * Look up an aspect by id. Returns the `Aspect` object (with `id`,
 * `name`, `description`, `icon`, `group` fields) or `null` for
 * `ASPECT_NONE` (id 0) and unknown ids. Throws on registry-build
 * failure.
 * @param {number} id
 * @returns {any}
 */
export function aspectInfo(id) {
    const ret = wasm.aspectInfo(id);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Look up the display label for an aspect by id in `lang`, falling
 * back to English. Returns `undefined` for `ASPECT_NONE` / unknown
 * ids or when no locale entry exists. Aspect locale entries are keyed
 * flat by the aspect's globally-unique `name` (see
 * `locales/aspects/en.json`); labels do NOT inherit along the
 * parent chain. Pairs with [`aspect_description`].
 * @param {number} id
 * @param {string} lang
 * @returns {string | undefined}
 */
export function aspectLabel(id, lang) {
    const ptr0 = passStringToWasm0(lang, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.aspectLabel(id, ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    let v2;
    if (ret[0] !== 0) {
        v2 = getStringFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v2;
}

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
 * @param {number} packed_def
 * @param {string} name
 * @returns {number | undefined}
 */
export function aspectValue(packed_def, name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.aspectValue(packed_def, ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === Number.MAX_SAFE_INTEGER ? undefined : ret[0];
}

/**
 * Look up a blueprint by its stable `u16` id. Returns the full
 * Blueprint object or `null` if the id isn't registered. Throws on
 * registry-build failure.
 * @param {number} id
 * @returns {any}
 */
export function blueprintById(id) {
    const ret = wasm.blueprintById(id);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Look up a blueprint by its source-key. Returns the full Blueprint
 * object or `null`. Throws on registry-build failure.
 * @param {string} key
 * @returns {any}
 */
export function blueprintByKey(key) {
    const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.blueprintByKey(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * **Legacy** — bit position (0..=31) of a card-flag by name, searched
 * across both `cards_state` and `cards_bk` fields (state first).
 * Returns `undefined` if no single-bit flag with that name exists in
 * either field. Ambiguous against the split-field schema — callers
 * that need to know which host integer the bit lives in should use
 * [`cardFlagBitIn`] with an explicit field name instead.
 * @param {string} name
 * @returns {number | undefined}
 */
export function cardFlagBit(name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.cardFlagBit(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === 0xFFFFFF ? undefined : ret[0];
}

/**
 * Bit position (0..=31) of a single-bit flag in a specific field.
 * `field` is `"cards_state"` or `"cards_bk"`. Returns `undefined` if
 * no single-bit flag with that name is declared in the given field.
 * Preferred over [`cardFlagBit`] for new call sites — explicit field
 * argument means lookups can't accidentally collide across fields.
 * @param {string} field
 * @param {string} name
 * @returns {number | undefined}
 */
export function cardFlagBitIn(field, name) {
    const ptr0 = passStringToWasm0(field, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.cardFlagBitIn(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === 0xFFFFFF ? undefined : ret[0];
}

/**
 * `(shift, width)` of a multi-bit flag field in a specific field.
 * Returns `undefined` if no multi-bit field with that name is
 * declared in the given field. Use the returned pair to mask:
 * `mask = ((1 << width) - 1) << shift`, value extract:
 * `(host >> shift) & ((1 << width) - 1)`.
 * @param {string} field
 * @param {string} name
 * @returns {Uint8Array | undefined}
 */
export function cardFlagFieldShape(field, name) {
    const ptr0 = passStringToWasm0(field, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.cardFlagFieldShape(ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    let v3;
    if (ret[0] !== 0) {
        v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v3;
}

/**
 * **Legacy** — read the value of a multi-bit card-flag field by
 * name, searching across both `cards_state` and `cards_bk` (state
 * first). Caller passes a single `flags` u32 that should be the
 * matching host integer; ambiguous against the split-field schema.
 * Prefer [`cardFlagFieldValueIn`] with an explicit field name for
 * new call sites.
 * @param {number} flags
 * @param {string} name
 * @returns {number | undefined}
 */
export function cardFlagFieldValue(flags, name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.cardFlagFieldValue(flags, ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === Number.MAX_SAFE_INTEGER ? undefined : ret[0];
}

/**
 * Field-routing helper for multi-bit fields — given both host
 * integers and a field name, returns the extracted value from
 * whichever field declares it (state-first lookup). Returns
 * `undefined` for unknown field names.
 * @param {number} state
 * @param {number} bk
 * @param {string} name
 * @returns {number | undefined}
 */
export function cardFlagFieldValueAny(state, bk, name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.cardFlagFieldValueAny(state, bk, ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === Number.MAX_SAFE_INTEGER ? undefined : ret[0];
}

/**
 * Read the value of a multi-bit field in a specific host integer.
 * `field` is `"cards_state"` or `"cards_bk"`; `host` is the value
 * of the corresponding `Card.flags_state` / `Card.flags_bk` column.
 * Returns `undefined` if no multi-bit field with that name is
 * declared in the given field.
 * @param {string} field
 * @param {number} host
 * @param {string} name
 * @returns {number | undefined}
 */
export function cardFlagFieldValueIn(field, host, name) {
    const ptr0 = passStringToWasm0(field, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.cardFlagFieldValueIn(ptr0, len0, host, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === Number.MAX_SAFE_INTEGER ? undefined : ret[0];
}

/**
 * Look up the display label for a packed definition in the given
 * language, e.g. `cardLabel(packed, "en")` → `"Log"`. Falls back to
 * English when `lang` has no entry. Returns `undefined` for unknown
 * packed ids or locale entries with no label. Throws on registry-build
 * failure. Callers should fall back to `def.key` on `undefined`.
 * @param {number} packed_def
 * @param {string} lang
 * @returns {string | undefined}
 */
export function cardLabel(packed_def, lang) {
    const ptr0 = passStringToWasm0(lang, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.cardLabel(packed_def, ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    let v2;
    if (ret[0] !== 0) {
        v2 = getStringFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v2;
}

/**
 * Look up a `card_type` id by name (e.g. `"soul"`, `"tile"`).
 * Returns `undefined` for unknown names. Source of truth
 * is `content/cards/types.json`. Used by JS-side code that needs to
 * branch on a card's type (without hard-coding the numeric id).
 * @param {string} name
 * @returns {number | undefined}
 */
export function cardTypeId(name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.cardTypeId(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === 0xFFFFFF ? undefined : ret[0];
}

/**
 * Decode a packed `(cardType:u4 | definitionId:u12)` value into a
 * `CardDefinition`-shaped JS object. Returns `null` if no card
 * matches the packed value. Throws a string error if the card
 * registry failed to build (malformed JSON, unknown aspects, etc.).
 * @param {number} packed
 * @returns {any}
 */
export function decodeDefinition(packed) {
    const ret = wasm.decodeDefinition(packed);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Look up a card's packed value by its bare key (e.g. `"fatigue"`).
 * Returns `undefined` if no card has that key. Throws on registry-build
 * failure.
 * @param {string} key
 * @returns {number | undefined}
 */
export function findPackedByKey(key) {
    const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.findPackedByKey(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === 0xFFFFFF ? undefined : ret[0];
}

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
 * @param {number} state
 * @param {number} bk
 * @param {string} name
 * @returns {boolean}
 */
export function hasCardFlag(state, bk, name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.hasCardFlag(state, bk, ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] !== 0;
}

/**
 * @returns {number}
 */
export function inventoryLayer() {
    const ret = wasm.inventoryLayer();
    return ret;
}

/**
 * Whether the given `cardType` id resolves to a hex-shaped type
 * (`"hex"` in `cards/types.json`). Throws on registry-build failure.
 * @param {number} type_id
 * @returns {boolean}
 */
export function isHexType(type_id) {
    const ret = wasm.isHexType(type_id);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] !== 0;
}

/**
 * Generic locale label lookup — wraps `locales_core::label(domain,
 * lang, path)` with the standard English fallback. Exposed for
 * client-only domains that don't warrant a dedicated wrapper (today
 * `panels`, whose strings never touch the sim). `domain` is the
 * `locales/<domain>/` folder name; `path` is the dotted entry path
 * (for panels, the flat panel key matching
 * `content/panels/defaults.json`). Returns `undefined` when neither
 * `lang` nor English registers the entry — callers fall back to the
 * bare key. Throws on registry-build failure.
 * @param {string} domain
 * @param {string} path
 * @param {string} lang
 * @returns {string | undefined}
 */
export function localeLabel(domain, path, lang) {
    const ptr0 = passStringToWasm0(domain, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(lang, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.localeLabel(ptr0, len0, ptr1, len1, ptr2, len2);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    let v4;
    if (ret[0] !== 0) {
        v4 = getStringFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v4;
}

/**
 * Generic locale variant lookup — wraps `locales_core::variant(domain,
 * lang, path, variant)` with English fallback. `variant` is the
 * dotted variant key; panels store each non-title string as a flat
 * variant, so the key is the bare string name (`"resetPanels"`,
 * `"inputPlaceholder"`, …). Returns `undefined` when the entry exists
 * but lacks the variant, or when neither language has the entry.
 * Throws on registry-build failure.
 * @param {string} domain
 * @param {string} path
 * @param {string} variant
 * @param {string} lang
 * @returns {string | undefined}
 */
export function localeVariant(domain, path, variant, lang) {
    const ptr0 = passStringToWasm0(domain, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(variant, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ptr3 = passStringToWasm0(lang, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len3 = WASM_VECTOR_LEN;
    const ret = wasm.localeVariant(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    let v5;
    if (ret[0] !== 0) {
        v5 = getStringFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v5;
}

/**
 * @param {number} card_type
 * @param {number} def_id
 * @returns {number}
 */
export function packDefinition(card_type, def_id) {
    const ret = wasm.packDefinition(card_type, def_id);
    return ret;
}

/**
 * @param {number} q
 * @param {number} r
 * @returns {bigint}
 */
export function packMacroZone(q, r) {
    const ret = wasm.packMacroZone(q, r);
    return BigInt.asUintN(64, ret);
}

/**
 * @param {number} local_q
 * @param {number} local_r
 * @param {number} x
 * @param {number} y
 * @returns {number}
 */
export function packMicroLoose(local_q, local_r, x, y) {
    const ret = wasm.packMicroLoose(local_q, local_r, x, y);
    return ret >>> 0;
}

/**
 * @param {number} local_q
 * @param {number} local_r
 * @returns {number}
 */
export function packMicroSnap(local_q, local_r) {
    const ret = wasm.packMicroSnap(local_q, local_r);
    return ret >>> 0;
}

/**
 * @param {bigint} time_ms
 * @param {number} sequence
 * @returns {bigint}
 */
export function packValidAt(time_ms, sequence) {
    const ret = wasm.packValidAt(time_ms, sequence);
    return BigInt.asUintN(64, ret);
}

/**
 * @param {number} card_type
 * @returns {number}
 */
export function packZoneDefinition(card_type) {
    const ret = wasm.packZoneDefinition(card_type);
    return ret;
}

/**
 * @returns {number}
 */
export function pocketDimensionLayer() {
    const ret = wasm.pocketDimensionLayer();
    return ret;
}

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
 * @param {number} id
 * @returns {any}
 */
export function recipeById(id) {
    const ret = wasm.recipeById(id);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Look up a recipe by its source-key (e.g. `"cut_tree"`,
 * `"strike_success"`). Returns the full Recipe IR serialized to JS
 * or `null` if no recipe with that key is registered. Throws on
 * registry-build failure.
 *
 * Used by callers that have a string key in hand — for example, a
 * card def's `magnetic.recipe` field, or recipe-name lookups in
 * debug tooling.
 * @param {string} key
 * @returns {any}
 */
export function recipeByKey(key) {
    const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.recipeByKey(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

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
 * @returns {any}
 */
export function recipesAll() {
    const ret = wasm.recipesAll();
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * @param {number} v
 * @returns {any}
 */
export function unpackDefinition(v) {
    const ret = wasm.unpackDefinition(v);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * @param {bigint} v
 * @returns {any}
 */
export function unpackMacroZone(v) {
    const ret = wasm.unpackMacroZone(v);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * @param {number} v
 * @returns {any}
 */
export function unpackMicroLoose(v) {
    const ret = wasm.unpackMicroLoose(v);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * @param {number} v
 * @returns {number}
 */
export function unpackZoneDefinition(v) {
    const ret = wasm.unpackZoneDefinition(v);
    return ret;
}

/**
 * @param {bigint} packed
 * @returns {bigint}
 */
export function validAtTime(packed) {
    const ret = wasm.validAtTime(packed);
    return BigInt.asUintN(64, ret);
}

/**
 * @returns {number}
 */
export function worldLayer() {
    const ret = wasm.worldLayer();
    return ret;
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_ef53bc310eb298a0: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_String_8564e559799eccda: function(arg0, arg1) {
            const ret = String(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_is_string_c236cabd84a4d769: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_throw_1506f2235d1bdba0: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_new_622fc80556be2e26: function() {
            const ret = new Map();
            return ret;
        },
        __wbg_new_ce1ab61c1c2b300d: function() {
            const ret = new Object();
            return ret;
        },
        __wbg_new_d90091b82fdf5b91: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_set_52b1e1eb5bed906a: function(arg0, arg1, arg2) {
            const ret = arg0.set(arg1, arg2);
            return ret;
        },
        __wbg_set_6be42768c690e380: function(arg0, arg1, arg2) {
            arg0[arg1] = arg2;
        },
        __wbg_set_dca99999bba88a9a: function(arg0, arg1, arg2) {
            arg0[arg1 >>> 0] = arg2;
        },
        __wbindgen_cast_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0) {
            // Cast intrinsic for `I64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./resonantdust_content_bg.js": import0,
    };
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('resonantdust_content_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
