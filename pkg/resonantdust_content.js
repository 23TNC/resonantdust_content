/* @ts-self-types="./resonantdust_content.d.ts" */

/**
 * Every registered texture definition, in stable-id order. Each entry
 * carries `id`, `cardType`, `aspectId`, `aspectName`, `object`,
 * `size`, and `scale: { min, max }`. Returns an empty array when no
 * textures are registered. Throws on registry-build failure.
 *
 * Called once at startup by `TextureRegistry.ts` to build the client-side
 * lookup map; not intended for per-frame use.
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
 * Bit position (0..=7) of a card-flag by name (e.g. `"drop_hold"`,
 * `"position_locked"`, `"dead"`). Returns `undefined` if no flag with
 * that name is declared in `cards/flags.json`. Throws on registry-build
 * failure. JS-side callers typically convert to a mask via
 * `1 << bit` before testing against `row.flags`.
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
 * Look up a `card_type` id by name (e.g. `"mini_zone"`, `"soul"`,
 * `"tile"`). Returns `undefined` for unknown names. Source of truth
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
 * @param {number} stacked_state
 * @param {number} surface
 * @returns {boolean}
 */
export function isStackLayout(stacked_state, surface) {
    const ret = wasm.isStackLayout(stacked_state, surface);
    return ret !== 0;
}

/**
 * @returns {number}
 */
export function miniZoneLayer() {
    const ret = wasm.miniZoneLayer();
    return ret;
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
 * @returns {number}
 */
export function packMacroZone(q, r) {
    const ret = wasm.packMacroZone(q, r);
    return ret >>> 0;
}

/**
 * @param {number} q
 * @param {number} r
 * @param {number} stacked_state
 * @returns {number}
 */
export function packMicroZone(q, r, stacked_state) {
    const ret = wasm.packMicroZone(q, r, stacked_state);
    return ret;
}

/**
 * @param {number} direction
 * @returns {number}
 */
export function packSlotMicroZone(direction) {
    const ret = wasm.packSlotMicroZone(direction);
    return ret;
}

/**
 * @param {number} position
 * @param {number} direction
 * @param {number} stacked_state
 * @returns {number}
 */
export function packStackMicroZone(position, direction, stacked_state) {
    const ret = wasm.packStackMicroZone(position, direction, stacked_state);
    return ret;
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
 * All starter packs registered for a given soul card key (e.g.
 * `"human"`). Returns an array of `StarterPack` objects (`id`,
 * `soul`, `packId`, `contents: [{cardKey, packedDefinition,
 * count}]`). Empty array for unknown soul keys. Throws on
 * registry-build failure.
 *
 * Used by the character-create panel to enumerate which packs the
 * player can pick from. JS-side filtering by soul is unnecessary
 * since this is already soul-scoped at the call site.
 * @param {string} soul
 * @returns {any}
 */
export function starterPacksForSoul(soul) {
    const ptr0 = passStringToWasm0(soul, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.starterPacksForSoul(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

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
 * @param {number} packed_def
 * @param {string} name
 * @returns {number | undefined}
 */
export function traitValue(packed_def, name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.traitValue(packed_def, ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] === Number.MAX_SAFE_INTEGER ? undefined : ret[0];
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
 * @param {number} v
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
export function unpackMicroZone(v) {
    const ret = wasm.unpackMicroZone(v);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * @param {number} v
 * @returns {any}
 */
export function unpackStackMicroZone(v) {
    const ret = wasm.unpackStackMicroZone(v);
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
        __wbg_Error_bce6d499ff0a4aff: function(arg0, arg1) {
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
        __wbg___wbindgen_throw_9c31b086c2b26051: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_new_02d162bc6cf02f60: function() {
            const ret = new Object();
            return ret;
        },
        __wbg_new_310879b66b6e95e1: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_set_6be42768c690e380: function(arg0, arg1, arg2) {
            arg0[arg1] = arg2;
        },
        __wbg_set_78ea6a19f4818587: function(arg0, arg1, arg2) {
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
