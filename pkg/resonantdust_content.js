/* @ts-self-types="./resonantdust_content.d.ts" */

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
 * Decode a packed `(cardType:u4 | cardCategory:u4 | definitionId:u8)` value
 * into a `CardDefinition`-shaped JS object. Returns `null` if no card
 * matches the packed value. Throws a string error if the card registry
 * failed to build (malformed JSON, unknown aspects, etc.).
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
 * @param {number} hex_def
 * @param {number} root_def
 * @param {Uint16Array} slot_defs
 * @param {number} direction
 * @param {Uint16Array} root_above
 * @param {Uint16Array} actor_above
 * @param {Uint16Array} root_below
 * @param {Uint16Array} actor_below
 * @returns {any}
 */
export function matchStackRecipe(hex_def, root_def, slot_defs, direction, root_above, actor_above, root_below, actor_below) {
    const ptr0 = passArray16ToWasm0(slot_defs, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray16ToWasm0(root_above, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passArray16ToWasm0(actor_above, wasm.__wbindgen_malloc);
    const len2 = WASM_VECTOR_LEN;
    const ptr3 = passArray16ToWasm0(root_below, wasm.__wbindgen_malloc);
    const len3 = WASM_VECTOR_LEN;
    const ptr4 = passArray16ToWasm0(actor_below, wasm.__wbindgen_malloc);
    const len4 = WASM_VECTOR_LEN;
    const ret = wasm.matchStackRecipe(hex_def, root_def, ptr0, len0, direction, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4);
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
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
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
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
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

let cachedUint16ArrayMemory0 = null;
function getUint16ArrayMemory0() {
    if (cachedUint16ArrayMemory0 === null || cachedUint16ArrayMemory0.byteLength === 0) {
        cachedUint16ArrayMemory0 = new Uint16Array(wasm.memory.buffer);
    }
    return cachedUint16ArrayMemory0;
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passArray16ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 2, 2) >>> 0;
    getUint16ArrayMemory0().set(arg, ptr / 2);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
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
    cachedUint16ArrayMemory0 = null;
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
