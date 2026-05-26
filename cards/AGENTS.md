# content/cards/

Card data — consumed by both the SpacetimeDB server (Rust) and the pixijs
client. Loaded by [`src/definition_core.rs`](../src/definition_core.rs).

## Layout

```
cards/
  types.json       # card_type registry (with shape + visibility)
  aspects.json     # aspect catalog (i32-valued, 1-indexed across groups)
  traits.json      # trait catalog  (f32-valued, 1-indexed across groups)
  flags.json       # bit-position registry for cards.flags u32
  id.json          # stable definition_id per (type, key) — generated
  data/            # card definition tree, auto-discovered by build.rs
    requisites/    # equipment.json, resources.json
    souls/         # human.json
    status/        # mental.json, stats.json
    tiles/         # forest.json, resources.json
  disciplines/     # LEGACY — old-format file, not auto-loaded
```

Everything under `data/` is auto-discovered recursively. Subdirectory
names are organisational — the loader doesn't care. **Display labels
are NOT in this tree** — they live under
[`../locales/cards/<lang>.json`](../locales/cards/).

## File format (cards/data/**/*.json)

Each file is a nested object: `card_type → card_key → spec`. The former
`category` middle level was retired — `packed_definition` is now
`[card_type:u4 | def_id:u12]`. The `categories` block in `types.json`
is tombstoned; the `card_type` key tree under `data/` and under
`cards/id.json` is flat (no category level).

```json
{
  "soul": {
    "human": {
      "style": ["#a8e0e6", "#ecd6aa", "#0b1426"],
      "aspects": { "mind": 2, "body": 2, "soul": 2, "skill": 1, "order": 1 },
      "traits":  { "speed": 12 }
    }
  }
}
```

One file can carry multiple types — every top-level key must be a
known `card_type` in [`types.json`](types.json). **Unknown types are
silently skipped** at registry build, so content files can outpace
the registry; a typo just won't produce a decodable card.

### Card spec fields

| Field | Required | Notes |
| --- | --- | --- |
| `style` | yes | `[string, string, string]` — three CSS hex colors (`#RRGGBB`, lowercase or uppercase, exactly 6 hex digits). Validated at registry build; invalid hex produces a stored registry error. Used by the renderer; the server doesn't otherwise interpret them. |
| `aspects` | no | Map `aspect_name → i32 value`. Every name must be declared in [`aspects.json`](aspects.json) — unknown aspects are a hard build error. Default `{}`. Static — every card row carrying this def sees the same numbers. |
| `traits` | no | Map `trait_name → number`. Every name must be declared in [`traits.json`](traits.json) — unknown traits are a hard build error. Values are parsed as `f64` and stored as `f32` (so `1`, `1.0`, and `1.2` all round-trip cleanly). Default `{}`. |
| `stock` | no | Array of row-mutable aspect slots — same aspect namespace, but values live on the row (tile bits) instead of the def. Each entry `{ "aspect": <name>, "max": 1..=3, "default": 0..=max, "mode"?: "count" \| "index" }`. Cap of 2 slots per def. Tile-only in v1. See [Stock slots](#stock-slots) below. |
| `object` | no | Foreground card art. `{ "aspect": <aspect-name>, "index"?: <n> }` — the renderer pulls a sprite from `pixijs/public/textures/cards/objects/<size>_<aspect>_pack/`. Without `index`, a deterministic per-row seed picks a variant (hash of `card_id`); with `index: N`, the `_N.png` variant is pinned. The aspect must be a declared render-only aspect in [`aspects.json`](aspects.json) (`{ size, anchor }` metadata). Replaces the retired `sprite` field. |
| `texture` | no | Card-body texture (PNG fill instead of `style[0]`). `{ "name": <texture-name> }` — the renderer pulls from `pixijs/public/textures/cards/textures/<size>_<name>/`. Same asset pipeline as `object`. |
| `lifecycle` | no | Defines a time-windowed transformation. `{ "recipe": <recipe-key>, "duration_ms": <u32>, "failure"?: <recipe-key> }`. At creation, cards with a `lifecycle:` block get `FLAG_LIFECYCLE_PENDING` stamped via def-flag inheritance (`magnetic` at bit 2 in [flags.json](flags.json) pending the rename). The client fires the success recipe via `propose_action` once inventory satisfies the slots, or the failure recipe at timer expiry. See [docs/LIFECYCLE_REWRITE.md](../../../docs/LIFECYCLE_REWRITE.md). |

The spec object must be present even if empty (just `{}`), but
`aspects` / `traits` / `stock` can be omitted entirely — most cards
declare none of them.

There is **no `name` / `display_name` field** in the card JSON. Display
labels are resolved at runtime via the locale registry — see
[Display labels](#display-labels) below.

### Aspects vs traits — when to use which

- **Aspects** are `i32`, aggregable across a stack (the action machinery
  sums aspect contributions across every claimed card in a chain). Used
  for recipe predicates (`{"aspect": "labor", "min": 1}`), duration
  tiers (more `fleeting` → faster expiry), and any "this much of X" axis.
- **Traits** are `f32`, scoped to a single card (`def.trait_value(id)`
  returns the card's own value or `None`). Used for non-aggregating
  per-card facts: a tile's movement `cost`, a soul's per-turn `speed`.
  Never summed across a chain.

If in doubt: does it make sense to add two cards' values together?
Aspect. If not: trait.

### Stock slots

`stock` declares **row-mutable aspect values** for tile defs — same
aspect namespace as `aspects`, but the actual value lives on the
zone tile's u2 bits rather than baked into the def. A forest tile
def declares `{ "aspect": "wood", "max": 3, "default": 2 }` and
every forest-tile *row* carries its own current wood value 0..=3
that recipes can decrement.

```json
"forest_1": {
  "style": [...],
  "stock": [
    { "aspect": "wood",  "max": 3, "default": 2 },
    { "aspect": "stone", "max": 1, "default": 0 }
  ]
}
```

Per-slot fields:

| Field | Notes |
| --- | --- |
| `aspect` | Name from [`aspects.json`](aspects.json). Unknown → hard build error. |
| `max` | Cap on the row value. `1..=3` (u2 storage). A def using a slot as a boolean sets `max: 1`. |
| `default` | Initial value worldgen / spawn paths seed the slot with. `0..=max`. |
| `mode` | Optional. `"count"` (default) renders `N` copies of the aspect's sprite at this slot. `"index"` picks a single sprite at `_<N>.png` from the pack, so the visible variant cycles as the stock value mutates (e.g. `_3 → _2 → _1 → (none)`). Falls through `ObjectTextureManager.get(..., index)` — the same resolver as the `object:` field. |

**Order matters.** Array index 0 maps to the per-tile `stock0` bits,
index 1 maps to `stock1`. Reordering the array is a data-breaking
change — every existing tile row would see its values swapped. Cap
of 2 slots per def (the per-tile u16 has `u4` reserved for stock).

**Same aspect can appear in both `aspects` and `stock`.** The
matcher prefers the row's stock value when evaluating
`{"aspect": <name>, "min": N}` against a tile that declares this
aspect in `stock`; the def's static `aspects` value falls back for
non-tile contexts (texture lookups, defs without a matching stock
slot).

Recipes consume stock via output ops `slot.0.0.aspect.<name>.sub: N`
(under the recipe-tape model) — see
[content/recipes/AGENTS.md](../recipes/AGENTS.md) for the recipe-side
shape.

## cards/types.json

```json
{
  "_rules": {
    "public_max_id":     3,
    "max_id":            15,
    "subscription_mask": "packed_definition < 0x4000 → public types only",
    "shapes":            ["rect", "hex"]
  },
  "types": {
    "requisite": { "id": 0, "visibility": "public",  "shape": "rect" },
    "tile":      { "id": 7, "visibility": "private", "shape": "hex"  },
    ...
  },
  "_categories_tombstone": "retired — packed_definition is now [type:u4 | def_id:u12]"
}
```

Per-type fields:

| Field | Notes |
| --- | --- |
| `id` | u4, `[0, 15]`. Goes into the high nibble of `packed_definition`. Append-only — never recycle an id. |
| `visibility` | `"public"` (id `[0, 3]`) or `"private"` (id `[4, 15]`). Enforced by SpacetimeDB subscription filters: public rows are visible cross-player for trade subscriptions; private rows are owner-only. The numeric cutoff lives in `_rules.public_max_id` and is materialised on the wire as `packed_definition < 0x4000`. |
| `shape` | `"rect"` or `"hex"`. Drives [`is_hex_type`](../src/definition_core.rs); rect types stack as chain roots, hex types are tile-shaped (occupy a hex). Both flavours are first-class cards under the unified card model — there's no separate "hex tile anchor" state. Missing field defaults to rect on the read path, but always declare it explicitly. |
| `_comment` | Optional — describes what the type is for. |

`_reserved_<n>` entries hold a numeric id slot for a future type
without registering it as real. Bucket keys like `"_reserved_1"` will
never resolve and the loader skips them.

The former `categories` block is tombstoned — `packed_definition`
collapsed `[type:u4 | category:u4 | def_id:u8]` into
`[type:u4 | def_id:u12]`, freeing 4 bits for a 16× larger def_id
namespace.

## cards/aspects.json

```json
{
  "resources": {
    "_comment": "Raw and processed materials",
    "labor": "Work or effort applied to a task",
    "wood":  "Timber — logs, branches, raw lumber",
    ...
  },
  "elements": { "earth": "...", "fire": "...", ... },
  ...
}
```

Top-level keys are organisational groups. Each group's children are
`aspect_name → description` (string). Aspect IDs are assigned at build
time in JSON insertion order **across all groups** (id 0 reserved as
`ASPECT_NONE`). The crate ships with `serde_json`'s `preserve_order`
feature enabled, so insertion order is the file order.

## cards/traits.json

Same shape as `aspects.json`, but values are `f32`. Today's catalog is
small:

```json
{
  "tile": {
    "cost": "Movement cost paid to enter the tile."
  },
  "soul": {
    "speed": "Movement allowance per turn."
  }
}
```

Trait IDs are 1-indexed across groups in insertion order (id 0
reserved as `TRAIT_NONE`).

## cards/flags.json

Bit-position registry for `Card.flags` (u32). Two entry shapes:

```json
{
  "cards": {
    "drop_hold":      { "bit": 3,   "description": "..." },
    "progress_style": { "bits": [8, 9, 10], "description": "..." }
  }
}
```

- `"bit": n` — single bit at position `n ∈ [0, 31]`. Surfaced via
  [`card_flag_bit`](../src/flags_core.rs) → `Option<u8>`. JS-side
  callers convert to a mask via `1 << bit`.
- `"bits": [low, …, high]` — contiguous multi-bit field, ascending.
  Surfaced via [`card_flag_field`](../src/flags_core.rs) →
  `Option<FlagField { shift, width }>`. `FlagField::pack(value)` and
  `FlagField::mask()` are the helpers; the JS API exposes the
  read-value path via `cardFlagFieldValue(flags, name)`.

Both shapes are append-only. Removed flags leave `_reserved_<bit>`
tombstones so the bit can't be silently reused for unrelated meaning.

The recipe parser resolves `{"flag": "<name>"}` entity predicates and
`set.start.<role>.<flag>` operations through this file — a typo'd
flag name is a hard build error.

The file also has `actions` / `magnetic_actions` sections kept as
historical context (those tables were retired in the magnetic rewrite
and don't exist server-side anymore).

## Stable definition IDs (cards/id.json)

```json
{
  "soul":      { "human": 1 },
  "tile":      { "forest_1": 1, "forest_2": 2, "tree": 3, "rock": 4 },
  "requisite": { "log": 1, "axe": 2 }
}
```

- IDs are **u12** (1–4095), scoped per `card_type`. **`0` is reserved
  as the sentinel "no card."** Flat per-type — the former
  `(type, category)` middle level is gone (see "categories tombstoned"
  above).
- Generated by [`../gen-ids.py`](../gen-ids.py). Default-mode runs
  reassign IDs on every invocation (fine while wire compatibility
  doesn't matter); `--skip-known` preserves existing ids and only
  appends new entries (the discipline you want once ids are baked
  into save data).
- The server combines `definition_id` with the bucket's `card_type`
  at build time to produce the wire-format `u16` `packed_definition`:

  ```
  bits 15..12 : card_type      (u4, from types.json)
  bits 11..0  : definition_id  (u12, from cards/id.json)
  ```

- [`find_packed_by_key(key) → Option<u16>`](../src/definition_core.rs)
  uses a flat `by_key` map populated from `id.json`, so recipe
  products that name cards by bare key (`"fatigue"`, `"log"`) resolve
  in O(log n) without scanning every bucket. When two types ever share
  a key, the lookup returns whichever was inserted last — use
  [`find_packed("type/key")`](../src/definition_core.rs) (which goes
  through `by_path`) to disambiguate.

## Display labels

Card display labels live in
[`../locales/cards/<lang>.json`](../locales/), path-keyed by
`<card_type>.<key>` matching the `cards/id.json` nesting:

```json
{
  "requisite": {
    "log": {
      "label": "Log",
      "description": { "simple": "Just a wooden log." }
    },
    "axe": { "label": "Axe" }
  }
}
```

Each leaf object has a required `label` and optional nested fields like
`description.simple` / `description.detailed`. Lookups go through
[`locales_core::label("cards", lang, path)`](../src/locales_core.rs)
with an English fallback. The wasm API exposes a one-shot
`cardLabel(packed, lang)` that builds the path via
[`card_locale_path`](../src/definition_core.rs) and dispatches.

Cards without a locale entry render as their bare `key` — fine for
prototyping, sloppy for shipping. `bin/content check` is the place to
enforce coverage.

## Conventions

- **Card keys**: lowercase `snake_case` (`"log"`, `"corpus"`,
  `"woodcutting_axe"` if it ever exists). Stable across renames of the
  display label. Keys cannot be `"any"` (reserved as the recipe-side
  wildcard sentinel) and cannot start with `@` (reserved as the
  recipe-side card-type-match prefix). Some current keys carry `+` /
  `-` suffixes (`corpus+`, `corpus-`) — that's fine, the parser only
  forbids the two reserved forms.
- **Display labels**: title case (`"Log"`, `"Corpus"`) in the locale
  file; not enforced, but recommended.
- **Style**: 7-character `#RRGGBB`. Validator is strict — `#fff`
  shorthand is rejected.
- **Aspects / traits**: every name referenced must already exist in
  `aspects.json` / `traits.json`. Adding one to a card without adding
  it to the catalog is a hard build error. The error message names the
  file, card key, and unknown name.
- **Cross-bucket key uniqueness**: not enforced by schema, but
  recommended. Recipe products use bare card keys; when two types
  share a key, `find_packed_by_key` returns whichever was inserted
  last — fine if intended, a silent surprise if not.

## Pitfalls

- **Card key not in `cards/id.json`**: hard build error. Run
  `bin/content check` (or `python content/gen-ids.py` directly) after
  adding a new card.
- **Bucket type/category name mismatch with `types.json`**:
  silently skipped. The whole bucket disappears from the registry. If
  a card "doesn't decode," check that the top-level key is the
  singular form declared in `types.json`, and that `cards/id.json`
  picked it up.
- **Definition_id overflow**: a single `card_type` bucket can't hold
  more than 4095 cards (u12 def_id). If you ever approach that, the
  bit layout has no headroom — `packed_definition` would need to widen.
- **Aspect / trait typos**: hard build errors. The error message names
  the file, card key, and unknown name — fix `aspects.json` /
  `traits.json` or the typo.
- **Color validation**: `"red"`, `"#fff"`, `"FFFFFF"` (no `#`) all
  fail. Always full `#RRGGBB`.
- **Duplicate aspect / trait on one card**: hard build error.
- **Reserved card keys**: `"any"` and any key starting with `@` would
  collide with recipe-side reserved sentinels. Today no card data
  trips this; if you ever add one, rename the card.
- **`cards/disciplines/`** holds a stale pre-rewrite-format file. It
  isn't loaded; treat the directory as legacy until cleaned up.

## Adding a new card

1. Pick the right `card_type` bucket in an existing file under
   [`data/`](data/), or add a new file anywhere under it.
2. Add the spec object — at minimum a `style`, plus optional
   `aspects` / `traits` / `stock` / `object` / `texture` /
   `lifecycle`.
3. Make sure every aspect / trait name in the spec already exists in
   `aspects.json` / `traits.json`. Add it there first if not.
4. Run `bin/content check` to assign a stable `definition_id` in
   `cards/id.json` and verify the registry builds.
5. Add a label to [`../locales/cards/en.json`](../locales/cards/en.json)
   under the matching `<type>.<key>` path. Without it the card renders
   as its bare key.

No `CARDS_FILES` constant to update — [`build.rs`](../build.rs)
auto-discovers files in `data/**/*.json`.

## Adding a new card type

1. Add a new entry to `types.json`'s `types` map. Pick an unused id
   in `[0, 0xF]`. Set `visibility` and `shape` deliberately.
2. Create or extend a bucket in `cards/data/**/*.json` whose top-level
   key matches the new singular name.
3. Run `bin/content check` to seed `cards/id.json` for the new type and
   verify the registry builds.
4. If the type is `"hex"`-shaped and should participate in stack
   chains, mirror it in the server-side stacking logic — the content
   crate only labels the shape; the server enforces what attaches to
   what.
