# content/recipes/

Recipe data — loaded by both the SpacetimeDB server and the pixijs
client. Parsing lives in [`../src/recipe_core.rs`](../src/recipe_core.rs).

## Layout

```
recipes/
  types.json     # recipe_type / recipe_category id registry
  id.json        # stable integer ID per (type, category, key) — generated
  data/          # recipe definition files, auto-discovered by build.rs
    01.json
    02.json
    ...
```

A recipe describes: **what shape of card configuration triggers it,
what it consumes, what it produces, and how long it takes**. The server
matches recipes against submitted stacks and freshly-created cards;
matched recipes start `Action`s that complete after the recipe's
duration. Magnetic outers install a ticker that pulls cards onto the
anchor until an inner recipe matches.

When multiple recipes match the same stack, the server picks the one
with the highest **priority weight** — see [Priority](#priority).

## File format

Each file is a three-level nested object: `recipe_type → category →
recipe_key → spec`. **The recipe's key in the JSON tree IS its id** —
there's no `id` field inside the spec object.

```json
{
  "stack": {
    "up": {
      "corpus_up": {
        "slots": ["corpus", "corpus"],
        "reagents": { "slots": [0] },
        "output": { "inventory": { "root": ["corpus-"] } },
        "duration": 10,
        "style": "rtl"
      }
    },
    "down": { ... }
  },
  "on_create": {
    "self":     { "fleeting": { ... } },
    "magnetic": { "despair":  { ... } }
  },
  "magnetic": {
    "up":   { "despair_success": { ... }, "despair_failure": { ... } },
    "down": { ... }
  }
}
```

Top-level keys are recipe types (`stack`, `on_create`, `magnetic`).
Second-level keys are categories (`up`/`down` for stack & magnetic;
`self`/`magnetic` for on_create). Missing buckets (omitted second-level
keys, or whole types) are valid and treated as `{}`. Unknown
`(type, category)` pairs are silently skipped — content can outpace
the Rust enum.

## Trigger types (recipes/types.json)

| Type | Category | Server-side `RecipeType` | Fired by |
| --- | --- | --- | --- |
| `stack` | `up` | `Stack(Up)` | `propose_action` (player submits a stack via `submit_inventory_stacks`) |
| `stack` | `down` | `Stack(Down)` | same, walking down |
| `on_create` | `self` | `OnCreate` | `insert_card_row` — the new card is both root and actor of the resulting action |
| `on_create` | `magnetic` | `OnCreateMagnetic` | `insert_card_row` — installs a magnetic ticker on the new card; outer carries `magnetic: { success, failure }` referencing inner recipes |
| `magnetic` | `up` | `Magnetic(Up)` | **lookup-only** — referenced by dotted path from an `OnCreateMagnetic` outer's `magnetic.{success,failure}`; the ticker dispatches between them |
| `magnetic` | `down` | `Magnetic(Down)` | same, with cards pulled below the anchor |

Stack recipes match by sliding the actor window along the chain trying
every position; the highest-weight match wins. `on_create.self`
matches the recipe's `root` (or `hex`) entity directly against the new
card. Magnetic inners are never matched against a chain — they're
named by their outer.

## Recipe spec fields

### Required-ish

| Field | Type | Notes |
| --- | --- | --- |
| `duration` | number or array | Fixed seconds, or a conditional tier list. See [Duration](#duration). Required on every recipe except `on_create.magnetic` outers, where it acts as the loop-budget cap rather than an action duration. |

### Common

| Field | Type | Notes |
| --- | --- | --- |
| `slots` | array of entities | Slot list. Slot `0` is the actor; subsequent slots fill outward along the matched branch. Empty / omitted for non-magnetic `on_create`. See [Entity grammar](#entity-grammar) below. |
| `root` | entity | Pre-condition on the chain root (`chain[0]`). For `on_create`: the entity the actor card must satisfy (one of `root` or `hex` is required for `on_create` recipes). For `stack` recipes: optional precondition on the stack root. When `root` is set, the actor's slot window is forced to start at `chain[1]+`; unset, the actor can start at `chain[0]`. Contributes to the [priority](#priority) **root tier** when satisfied. |
| `hex` | entity | Pre-condition on the hex card the chain root is attached to. The matcher resolves the hex from a rect-on-hex root (`stacked_state == OnHex`); the hex is sourced from the `cards` table when `micro_location` points at a real `Card` row, or from the `zones` packed cell at `(root.macro_zone, root.micro_zone q/r)` otherwise. Contributes to the **hex tier** of priority — top of the lex order, so a satisfied `hex` outranks any `root`/`slot` combination. Re-checked at completion as defense-in-depth: a chain that drifted off its hex refuses to produce. |
| `reagents` | object | What dies at completion. See [Reagents](#reagents). |
| `has` | object | Soul-stack feasibility predicates *above* the actor's / root's soul card. See [Has predicates](#has--has_below). |
| `has_below` | object | Same, below the soul card. |
| `output` | object | Cards produced on success. Nested `place → owner → [entities]`. See [Output targets](#output-targets). |
| `style` | string | Client render hint: `"none"` (0), `"ltr"` (1), `"rtl"` (2). Mapped to the `progress_style` 3-bit field on the actor card's completion row. Future styles slot into 3..=7. Omitted reads as `"none"`. |
| `set` | object | Flag manipulations applied at action start, scoped per role. See [`set`](#set--starting-flag-deltas). |

### Magnetic outers only (`on_create.magnetic`)

| Field | Type | Notes |
| --- | --- | --- |
| `magnetic` | object | `{ "success": "magnetic.<dir>.<key>", "failure": "magnetic.<dir>.<key>" }` — dotted paths referencing inner recipes filed under `magnetic.up.*` / `magnetic.down.*`. The leading `magnetic.<dir>.` prefix is required because recipe keys can collide across direction buckets. Resolved in a second pass after every recipe is registered. |
| `interval` | u32 | Seconds between ticks of the slot-fill loop. |
| `delay` | u32 | Seconds before the first tick fires. |
| `output_failure` | object | Cards produced when the loop runs out of budget (the outer's `duration`) without an inner ever matching. Same shape as `output`. Setting this on a non-magnetic recipe is a hard build error. |

`duration` on a magnetic outer is the **loop budget** — the action
self-destructs after that many seconds without dispatching, firing
`output_failure`. `delay` / `interval` may be omitted (they default to
implementation-defined values on the server).

## Entity grammar

An **entity** is a condition tree. Used for slot constraints (does this
card fit?), `root` / `hex` preconditions, `has` predicates, duration
`when` clauses, and product entities in `output` / `output_failure`.

### Bare-string sugar

| Form | Meaning |
| --- | --- |
| `"<card_key>"` | Card with this exact key (e.g. `"corpus"`). |
| `"any"` | Wildcard — matches any card. Lowest specificity. |
| `"@<type_name>"` | Any card whose `card_type` resolves to the named type via `cards/types.json`. Unknown type name is a hard build error. |

### Bare-array sugar

A JSON array is an **OR-list**: `["axe", "pickaxe"]` matches either.
Single-element arrays unwrap to the inner entity. Empty arrays are an
error.

### Tagged-object grammar

Dispatched by the first recognised key. Open-ended — new predicates
land as new `Entity` variants plus a parse arm.

| Key | Shape | Meaning |
| --- | --- | --- |
| `card` | `{"card": "<key>"}` | Explicit card key. Same effect as the bare string; useful when the key shadows a sentinel. |
| `aspect` | `{"aspect": "<name>", "min": N}` | Aspect check: card has `aspects[name] >= N`. `min` defaults to 1 if omitted. Name must exist in `aspects.json`. |
| `category` | `{"category": "<name>"}` | Card category match. |
| `flag` | `{"flag": "<name>"}` | Card carries this single-bit flag (resolved through `cards/flags.json`). |
| `any` | `{"any": true}` | Explicit wildcard. `{"any": false}` is an error. |
| `and` | `{"and": [entity, ...]}` | Conjunction — every child must match. |
| `or` | `{"or": [entity, ...]}` | Disjunction — same as bare-array sugar, useful for explicitness inside tagged trees. |
| `not` | `{"not": <entity>}` | Negation — child must NOT match. |

There is no per-spec weighted-OR object today; `WeightedOr` exists in
the Rust enum for future product-side selection but isn't surfaced in
the JSON grammar yet.

### Reserved string sentinels

Card keys cannot be `"any"` (wildcard) or start with `@` (type prefix).
Today's data trips neither.

## Reagents

What the recipe consumes on completion. Object form:

```json
"reagents": {
  "slots":      [0, 1],
  "roles":     ["root", "hex"],
  "has":        { "root": [...], "actor": [...] },
  "has_below":  { "root": [...], "actor": [...] }
}
```

All four keys optional; absent reagents = nothing consumed.

| Field | Type | Notes |
| --- | --- | --- |
| `slots` | array of u8 | **0-indexed** slot positions. `0` is the actor; `1`, `2`, … are non-actor slot fillers in order. |
| `roles` | array of strings | Named referents: `"root"` (the chain root's card) and `"hex"` (the hex card the chain is anchored to). |
| `has` / `has_below` | object | Same shape as the top-level [`has` / `has_below`](#has--has_below) — soul-stack matches that the recipe also wants to consume on completion. Each entry is one card pulled from the soul stack. |

### Stack-recipe consumption caveat

For `stack` recipes, the chain root isn't held in `CardHold` (leaving
it unheld is what lets multiple recipes share one root concurrently)
and isn't recoverable from server state at completion time. So
`roles: ["root"]` on a `stack` recipe is currently a no-op. Use
`slots: [0]` to consume the actor instead. When world layers land and
the chain root gets a server-side representation, root consumption
will be reconnected.

For `on_create.self`, `roles: ["root"]` and `slots: [0]` both refer to
the same card (root == actor) and either works.

## `has` / `has_below`

Feasibility predicates against the player's **soul stack** — the cards
stacked on the actor's or root's soul card, split by direction (above
vs below).

```json
"has": {
  "root":  [<entity>, <entity>],
  "actor": [<entity>]
}
```

Each entry is one slot that must be filled by *some* card in the
relevant soul-stack pool. The matcher prunes recipes whose has-list
can't find a candidate; satisfied has-predicates also feed into
reagent consumption when listed under `reagents.has` / `reagents.has_below`.

| Field | Pool |
| --- | --- |
| `has.root` | cards stacked **above** the chain root's soul card |
| `has.actor` | cards stacked **above** the actor's soul card |
| `has_below.root` | cards stacked **below** the chain root's soul card |
| `has_below.actor` | cards stacked **below** the actor's soul card |

Up vs down here matches the directional convention on the soul stack:
UP = equipment / above the soul, DOWN = action stack / below.

The wasm-side stack-matcher takes these pools as four separate
`Vec<u16>` arguments (`rootAbove`, `actorAbove`, `rootBelow`,
`actorBelow`) — recipes that declare a has-predicate the pool can't
satisfy are filtered out before scoring.

## `set` — starting flag deltas

```json
"set": {
  "start": {
    "hex":  { "drop_locked": true, "surface_locked": true },
    "root": { "drop_hold": true },
    "slot": { "...": false }
  }
}
```

Flag manipulations applied at action start, scoped per role. `true` =
force-on, `false` = force-off, omitted = untouched. Roles: `root`,
`slot`, `hex`. Currently only `set.start` is recognised; other timing
keys (`end`, …) are reserved.

The flag name must exist in [`../cards/flags.json`](../cards/flags.json)
as a single-bit entry. Multi-bit fields aren't settable through
`set.start`. The literal flag name `dead` is rejected — dying is a
server-internal state transition, not a recipe-declarable effect.

## Duration

Two shapes:

```json
"duration": 30
```

Fixed: 30 seconds.

```json
"duration": [
  { "when": { "aspect": "fleeting", "min": 4 }, "seconds": 20 },
  { "when": { "aspect": "fleeting", "min": 3 }, "seconds": 15 },
  { "when": { "aspect": "fleeting", "min": 2 }, "seconds": 10 },
  { "seconds": 5 }
]
```

Conditional: an array of tier objects. Each tier has `seconds`
(required, u32) and an optional `when` (an entity tree). The first
tier whose `when` is satisfied wins, in declaration order. The
**trailing tier must omit `when`** to act as the unconditional
fallback — a tier list without a fallback is a build error.

Conditions are evaluated against a single candidate card (today the
actor; future work may broaden this). `Card`-form and `"@type"`-form
entities work; aspect predicates check the candidate's aspect map;
`"any"` always satisfies.

## Output targets

```json
"output": {
  "<place>": {
    "<owner>": [<entity>, <entity>, ...]
  }
}
```

Fires on normal completion. Two-axis nested map: outer key picks the
**place**, inner key picks the **owner**. Each leaf is an array of
entities — one output card per entry; arrays / tagged objects pick one
alternative.

`output_failure` has the same shape and only applies to
`on_create.magnetic` outers (fires when the magnetic loop budget runs
out without dispatching).

### Place

| Place | Resolves to |
| --- | --- |
| `inventory` | The owner's inventory panel (layer 1, scoped by `macro_zone == owner_player_id`). |
| `location` | The world cell. Currently only `location.hex` is supported — drops the output entity onto the resolved hex tile. |

### Owner

| Owner | Resolves to |
| --- | --- |
| `root` | The chain root's holder. For inventory recipes, this is the player whose panel the chain root sits in. (For `stack` recipes today, the chain root isn't tracked server-side at completion — falls back to the actor's holder. Fine for the inventory POC.) |
| `actor` | The actor's holder (`actor.macro_zone`). |
| `hex` | The hex card's owner (`hex.owner_id`). At completion the server walks from the actor toward the chain root via `micro_location`; if the chain ends in a rect-on-hex root, the hex pointed to is resolved (from a `Card` row first, falling back to a `Zone` packed cell). Falls back to the actor's panel if the chain isn't on a hex, the hex is unowned, or the hex resolved from a `Zone` cell (which carries no per-cell ownership). |
| `action` | The action itself (used for cards that belong to the in-flight action, not yet exposed in normal play). |

`location.<owner>` only supports `hex` today; the parser rejects other
owners under `location`.

## Priority

When multiple recipes match the same stack, the server picks the
**highest-weight** match. Weight is a lex-ordered triple — comparison
stops at the first non-equal tier:

```
( hex_weight , root_weight , slot_weight )
```

### Tier ordering (strict)

1. **Hex tier** — any recipe whose `hex` is satisfied beats any
   recipe whose `hex` isn't.
2. **Root tier** — among recipes with equal hex weight, the one whose
   `root` is satisfied wins.
3. **Slot tier** — among recipes with equal hex and root weight, the
   sum of per-slot weights breaks the tie.

Recipes that don't declare a tier contribute `0` for that tier — so a
recipe with no `hex` always loses the hex tier to any recipe that has
a satisfied `hex`, regardless of how many slots either has.

### Per-leaf entity weights

When an entity matches a card, the leaf weight is:

| Entity form | Weight |
| --- | --- |
| `Card` (`"key"` or `{"card": ...}`) | **4** |
| `Aspect` (`{"aspect": ..., "min": ...}`) | **3** |
| `Category` (`{"category": ...}`) | **3** |
| `Type` (`"@type"`) | **2** |
| `Flag` (`{"flag": ...}`) | **2** |
| `Any` | **1** |
| `Not` (when satisfied — child does NOT match) | **1** |

### Composite weights

| Composite | Weight when matched |
| --- | --- |
| `And` (`{"and": [...]}`) | Sum of children's weights — every child must match. |
| `Or` (`["a", "b"]` or `{"or": [...]}`) | Max over children's weights — the highest-specificity branch that matched. |

### Worked example: card-key beats aspect

```json
{
  "stack": {
    "up": {
      "corpus_up_aspect": {
        "slots": [{"aspect": "corpus", "min": 1}, "corpus"],
        "reagents": {"slots": [0]},
        "duration": 10
      },
      "corpus_up": {
        "slots": ["corpus", "corpus"],
        "reagents": {"slots": [0]},
        "output": {"inventory": {"root": ["corpus-"]}},
        "duration": 10
      }
    }
  }
}
```

Both match a stack of two corpus cards. Tier weights:

| Recipe | hex | root | slot 0 | slot 1 | slot_weight | Total |
| --- | --- | --- | --- | --- | --- | --- |
| `corpus_up_aspect` | 0 | 0 | Aspect → 3 | Card → 4 | 7 | (0, 0, 7) |
| `corpus_up` | 0 | 0 | Card → 4 | Card → 4 | 8 | (0, 0, 8) |

`corpus_up` wins (8 > 7) — the more specific actor slot dominates. The
`_aspect` recipe acts as a fallback for stacks where the actor isn't
literally a corpus card but still has the corpus aspect.

### Tiebreak

If two recipes produce identical match weights, the matcher keeps the
**first-encountered** match (registry-declaration order: across files
in alphabetical-path order from `build.rs`'s sorted glob, then within
each file's JSON nesting). This is mostly relevant for catch-all
"fallback" recipes — declare them later in the file so a more specific
recipe always wins on ties.

### Where this is implemented

The priority evaluation lives **on both sides of the wire** and must
produce identical results.

- **Server (Rust)** — [`actions.rs`](../../spacetime/server/spacetimedb/src/actions.rs)
  drives the matcher per-actor; the scoring core is
  [`entity_specificity`](../src/recipe_core.rs) and
  [`match_stack_recipe_detail`](../src/recipe_core.rs). The server is
  the **authoritative** evaluator: when the client commits a stack,
  the server independently re-runs the priority calculation against
  the submitted card_ids and starts whichever action wins.
- **Client (TypeScript)** — runs the same evaluation locally via the
  wasm `matchStackRecipe` export ([`wasm_api.rs`](../src/wasm_api.rs))
  as a **pre-filter** to decide *whether to send the stack at all*. If
  no recipe would match (or the match is one the server is already
  running), the client doesn't bother committing. Has-predicate pools
  are passed in as four `Vec<u16>` arguments so the feasibility filter
  runs client-side too.

This dual implementation buys two independent things at once:

- **Efficiency.** The client doesn't waste a round-trip on every
  drag-drop — only when a commit would actually change server-side
  state.
- **Security.** The server doesn't blindly trust the client's view of
  "which recipe should fire." A malicious client claiming a specific
  outcome can't manufacture one; the server scores every matching
  recipe itself and picks the highest-weight winner. The client
  pre-filter is purely *advisory*.

Both sides read the same registry, so the **input** to evaluation is
automatically in sync — but when you change anything about priority
semantics (adding an entity form, changing weight constants, tweaking
composite rules, extending the `hex` tier), **change both
implementations in the same commit**.

Drift between them is a real bug class:

- Client predicts recipe A → sends stack → server picks recipe B (or
  nothing). UX glitch: the player saw one outcome predicted and got
  another.
- Client predicts no match → doesn't send → server *would* have matched
  something. Silent: the action never starts at all.

When debugging a "recipe didn't fire" report, first check whether both
sides agree about whether anything *should* have fired given the
submitted stack — divergence between client prediction and server
behavior is usually the smoking gun.

### Upgrade rule (running actions vs. new submissions)

When a stack already has a running action, a new submission doesn't
automatically restart the matcher. The server applies these rules
per-actor (every card in the submitted branch chain is evaluated as a
potential actor):

| Current action | Best recipe over visible window | Outcome |
| --- | --- | --- |
| none | none | nothing |
| none | `r` | start `r` |
| `a` | none | cancel `a` |
| `a` | same recipe AND slot fillers unchanged | keep `a` running (a drifted root that still satisfies `recipe.root` is invisible to the action — nothing to update) |
| `a` | different recipe, or slot fillers moved | cancel `a`, start the winner |

**Slot fillers are strict.** The cards in the slot window must be the
same identities as the action's currently-claimed slot fillers (set
equality on `CardHold`s — the claim is exactly actor + slot fillers,
no root to subtract). Any swap, removal, or replacement cancels the
action — even when the same recipe ID still matches the new
arrangement.

**The chain root is fluid and unheld.** The chain root is *not* in
`CardHold` — leaving it free is what lets multiple recipes share one,
e.g. `[attack, sword] + human` over the top branch and
`[heal, anima] + human` over the bottom running concurrently. The
matcher re-validates `recipe.root` against the current chain root on
every upgrade pass; a drifted-but-still-matching root keeps the action
running, and a root that no longer matches cancels it.

Two practical consequences for recipe authors:

- A no-op submission (same stack, same arrangement) **does not reset
  the action timer**.
- A "better" recipe at the same actor position pre-empts a worse one
  immediately. The lex-ordered priority weights decide which is
  better.

Full mechanics, including the visible-chain walk and the
defense-in-depth check at completion, are documented in
[`spacetime/docs/recipe-upgrade.md`](../../spacetime/docs/recipe-upgrade.md).

## Stable recipe IDs (recipes/id.json)

Recipe keys are mapped to stable integer ids in a three-level nested
file:

```json
{
  "stack": {
    "up":   { "triple_corpus": 1, "dread_remover": 2, "corpus_up": 3, "cut_tree": 4, "break_rock": 5 },
    "down": { "corpus_down_aspect": 1, "corpus_down": 2 }
  },
  "on_create": {
    "self":     { "corpus-": 1, "fleeting": 2 },
    "magnetic": { "despair": 1, "strike": 2 }
  },
  "magnetic": {
    "up": { "despair_success": 1, "despair_failure": 2, "strike_success": 3, "strike_failure": 4 }
  }
}
```

IDs are scoped per `(recipe_type, recipe_category)` — each bucket
counts from 1 independently. The on-wire `Action.recipe` is
`pack_recipe(type_id, category_id, recipe_id)`:

```
bits 15..13 : recipe_type     (u3, from recipes/types.json)
bits 12..10 : recipe_category (u3, from recipes/types.json)
bits  9..0  : recipe_id       (u10, from recipes/id.json)
```

So one bucket can hold 1023 recipes; both `recipe_type` and
`recipe_category` are 3-bit fields.

`id.json` is generated by [`../gen-ids.py`](../gen-ids.py). Default
mode reassigns ids on every run; pass `--skip-known` to preserve
existing ids and only assign new ones (the discipline you want once
ids are baked into save data or persisted action rows). The registry
refuses to load a recipe whose `(type, category, key)` isn't in
`id.json` with `not found in recipes/id.json — run gen-ids.py`.

Recipe keys must be unique **across the whole file** — `gen-ids.py`
errors out if two buckets ever share a key.

## Worked examples (from `data/`)

### `corpus_up` — basic up-stack recipe

```json
"stack": { "up": {
  "corpus_up": {
    "slots": ["corpus", "corpus"],
    "reagents": { "slots": [0] },
    "output": { "inventory": { "root": ["corpus-"] } },
    "duration": 10,
    "style": "rtl"
  }
}}
```

Two corpus cards stacked → 10 second action → produces a `corpus-`
card to the root's panel, consumes the actor (slot 0, the lower
corpus). The upper corpus survives.

### `cut_tree` — hex-anchored with equipment check

```json
"stack": { "up": {
  "cut_tree": {
    "hex": "tree",
    "slots": ["corpus"],
    "has": { "actor": ["axe"] },
    "reagents": { "slots": [0], "roles": ["hex"] },
    "output": { "inventory": { "actor": ["corpus-", "log"] } },
    "duration": 10,
    "style": "rtl"
  }
}}
```

Triggers when the player submits a corpus card on a chain anchored to
a `tree` hex, *and* the actor's equipment soul-stack contains an
`axe`. 10 second action; on completion produces `corpus-` and `log` to
the actor's inventory, consumes the actor (`slots: [0]`) and the
`tree` hex (`roles: ["hex"]`).

### `fleeting` — on_create with conditional duration

```json
"on_create": { "self": {
  "fleeting": {
    "root": { "aspect": "fleeting", "min": 1 },
    "reagents": { "roles": ["root"] },
    "duration": [
      { "when": { "aspect": "fleeting", "min": 4 }, "seconds": 20 },
      { "when": { "aspect": "fleeting", "min": 3 }, "seconds": 15 },
      { "when": { "aspect": "fleeting", "min": 2 }, "seconds": 10 },
      { "seconds": 5 }
    ],
    "style": "rtl"
  }
}}
```

Triggers on any new card with `fleeting >= 1`. Duration scales with
the fleeting value (20s/15s/10s/5s fallback). Consumes the card on
completion (via `roles: ["root"]`, which here resolves to the actor
since root == actor for `on_create.self`); no `output` means the card
just expires.

### `despair` outer + `despair_success` / `despair_failure` inners

The magnetic-outer pattern: an event card is created (`despair`), which
installs a ticker on that hex. Every `interval` seconds the ticker
checks whether any inner recipe's slot list can be filled from the
player's cards stacked above the anchor. Within `duration` seconds, if
an inner ever matches, the success branch fires; if the budget
expires, the failure branch fires.

```json
"on_create": { "magnetic": {
  "despair": {
    "hex": "despair",
    "set": { "start": { "hex": { "drop_locked": true, "surface_locked": true } } },
    "magnetic": {
      "success": "magnetic.up.despair_success",
      "failure": "magnetic.up.despair_failure"
    },
    "duration": 60,
    "delay": 10,
    "interval": 2,
    "style": "ltr"
  }
}},

"magnetic": { "up": {
  "despair_success": {
    "hex": "despair",
    "root": "dread",
    "reagents": { "roles": ["hex", "root"] },
    "set": { "start": { "root": { "drop_hold": true } } },
    "output": { "inventory": { "hex": ["corpus"] } },
    "duration": 10,
    "style": "ltr"
  },
  "despair_failure": {
    "hex": "despair",
    "reagents": { "roles": ["hex"] },
    "output": { "inventory": { "hex": ["dread"] } },
    "duration": 10,
    "style": "ltr"
  }
}}
```

The outer locks the hex (`set.start.hex.drop_locked`,
`surface_locked`). The success inner fires when a `dread` card lands
above the `despair` hex within the 60-second budget — consumes both,
produces a `corpus` to the hex owner. The failure inner fires if the
budget expires — consumes the hex, produces a `dread` to the hex
owner (the despair "fed back" into the player's inventory).

## Conventions

- **Recipe keys** are `lowercase_snake_case`, descriptive of the
  recipe's effect or trigger. Globally unique (the build errors on
  collisions, including across direction buckets).
- **Slot 0 is the actor.** State this in mental shorthand: "slot 0 +
  slot 1 + …".
- **Conditional duration ordering**: write tiers from highest threshold
  to lowest, with the unconditional fallback last. The matcher walks
  them in order and takes the first that satisfies.
- **`reagents.slots: [0]` is the most common shape** — most recipes
  consume the actor and leave the rest of the stack alone.
- **Catch-all recipes go later in the file** — when two recipes
  produce identical match weights, declaration order is the tiebreak.
  Specific-then-general ordering keeps the more specific recipe
  preferred.
- **Magnetic inner naming**: by convention, inners are named
  `<outer>_success` / `<outer>_failure`. The `magnetic.success` /
  `magnetic.failure` paths on the outer point at them directly.

## Pitfalls

- **Recipe key not in `recipes/id.json`**: hard build error. Run
  `bin/content check` (or `python content/gen-ids.py`) after adding
  a new recipe.
- **Duplicate `key` across files / buckets**: hard build error.
- **Unknown aspect / type / flag name in any entity**: hard build
  error at registry load — error message names file, recipe key, and
  path.
- **Missing fallback in conditional duration**: build error. The
  trailing tier MUST omit `when`.
- **`output_failure` on a non-magnetic recipe**: hard build error.
- **`magnetic` ref on a non-`on_create.magnetic` recipe**: hard build
  error.
- **`interval` / `delay` outside of `on_create.magnetic` outers**:
  hard build error.
- **`on_create` recipe missing both `root` and `hex`**: hard build
  error — an on_create recipe with neither would never match.
- **`set.start.<role>.dead: true`**: hard build error. Dying is
  server-internal.
- **`set.start` referencing an unknown flag name**: hard build error.
- **`reagents.roles: ["root"]` on a `stack` recipe**: silently a no-op
  (the chain root isn't held by the action). Use `slots: [0]` to
  consume the actor instead. Reconnects when world layers land.
- **`location.<owner>` for any owner other than `hex`**: hard build
  error.
- **`hex` field requires a rect-on-hex root** (`stacked_state == OnHex`
  on chain[0]). Inventory chains never satisfy a hex precondition;
  the field is meaningful only for world stacks where rectangles are
  anchored on hex cards. The hex itself can be a full `Card` row or a
  packed `Zone` cell — the matcher tries `cards` first, then falls
  back to `zones`.

## Adding a new recipe

1. Pick the file (or add a new one anywhere under [`data/`](data/)).
2. Insert the recipe under the right `<type>.<category>.<key>` path.
   Order within a bucket affects priority tiebreaks — declare specific
   recipes before catch-all ones.
3. Confirm every aspect name in entities exists in
   `../cards/aspects.json`.
4. Confirm every type name in `"@<type>"` entities exists in
   `../cards/types.json`.
5. Confirm every flag name in `{"flag": ...}` predicates or
   `set.start` ops exists in `../cards/flags.json`.
6. Confirm every card key in `output` / `output_failure` / bare-string
   entities exists in `../cards/data/**/*.json`.
7. Run `bin/content check` to assign a stable ID in `recipes/id.json`
   and verify the registry builds.
8. Add a label to [`../locales/recipes/en.json`](../locales/recipes/en.json)
   under the matching `<type>.<category>.<key>` path. Recipes with no
   locale entry will render as their bare key.

No `RECIPES_FILES` constant to update — [`build.rs`](../build.rs)
auto-discovers files in `data/**/*.json`.
