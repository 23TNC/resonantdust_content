# data/recipes/

Recipe data — loaded by both the SpacetimeDB server and the pixijs client.

## Layout

```
recipes/
  types.json     # recipe_type / recipe_category id registry
  flags.json     # bit-position registry for actions.flags
  id.json        # stable integer ID per (type, category, recipe-id) — generated
  data/          # recipe definition files
    01.json
    ...
```

Every file in [`data/`](data/) is a top-level array of **bucket
objects**. Each bucket pins a `type` and lists its recipes under one or
more direction-keyed arrays.

A recipe describes: **what shape of card configuration triggers it, what
it consumes, what it produces, and how long it takes**. The server matches
recipes against submitted stacks and freshly-created cards; matched
recipes start `Action`s that complete after the recipe's duration.

When multiple recipes match the same stack, the server picks the one
with the highest **priority weight** — see [Priority](#priority).

## File shape

The top-level array contains one or more buckets. A bucket has a `type`
field and one direction-keyed array of recipe records per direction
the type allows:

| Bucket `type` | Direction keys | What that direction means |
| --- | --- | --- |
| `"stack"` | `"up"` and/or `"down"` | Recipes that fire when the player submits a stack via `submit_inventory_stacks`, walking the chain in that direction from the submitted root. |
| `"on_create"` | `"self"` | Recipes that fire when a card is inserted via `insert_card_row` and match the new card against the recipe's `root` entity. |

```json
[
  {
    "type": "stack",
    "up": [
      {
        "id": "woodcutting",
        "slots": [[["labor", 1], [], ["wood", 1]], ["corpus"]],
        "reagents": [1],
        "output_success": {
          "inventory": {
            "root":  ["fatigue"],
            "actor": [["log", [4, 1], "vigor"]]
          }
        },
        "duration": 30,
        "style": ["rtl", "#ecd6aa", "#9b927d"]
      }
    ],
    "down": [ /* … */ ]
  },
  {
    "type": "on_create",
    "self": [
      {
        "id": "fatigue",
        "root": ["fatigue"],
        "reagents": [0],
        "output_success": { "inventory": { "actor": ["corpus"] } },
        "duration": 30,
        "style": ["rtl", "#9b927d", "#ecd6aa"]
      }
    ]
  }
]
```

The recipe records themselves no longer carry a `type` field — the
surrounding bucket and direction key together determine the recipe's
trigger.

### Required fields per recipe record

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Unique across **all** recipe files. Must also appear in [`id.json`](id.json) (see [Stable recipe IDs](#stable-recipe-ids)). The on-wire `Action.recipe` is the stable integer from `id.json`. |
| `slots` | array | Slot list. Slot 1 is the actor; subsequent slots fill outward along the matched branch. Empty for non-magnetic `on_create`. See *Entity grammar* for what each entry can be. |
| `reagents` | array | 1-indexed slot positions consumed on completion. `0` means "the chain root" (only meaningful when `root` is set, or for `on_create` where actor == root). Slot 1 is the actor. |
| `duration` | number or array | Fixed seconds, or a conditional list. See *Duration*. |

### Optional fields per recipe record

| Field | Type | Notes |
| --- | --- | --- |
| `root` | entity | Pre-condition on the chain root (`chain[0]`). For `on_create`: the entity the actor card must satisfy (required in practice — an `on_create` recipe without `root` never matches). For `stack` recipes: optional precondition on the stack root. When `root` is set, the actor's slot window is forced to start at `chain[1]+`; unset, the actor can start at `chain[0]`. Contributes to the [priority](#priority) **root tier** when satisfied. |
| `hex` | entity | Pre-condition on the hex card the chain root is attached to. The matcher resolves the hex from a rectangle root with `stacked_state == 3` and scores this entity against the hex card's def. The hex is sourced from the `cards` table when `root.micro_location` points at a real `Card` row, or from the `zones` packed cell at `(root.macro_zone, root.micro_zone q/r)` otherwise — so this works for hexes that are full `Card`s and for tiles that only live as packed `Zone` cells. Contributes to the **hex tier** of priority — top of the lex order, so a satisfied `hex` outranks any `root`/`slot` combination. Re-checked at completion as defense-in-depth: a chain that drifted off its hex (or onto a different hex) refuses to produce. |
| `magnetic` | string | `"top"` or `"bottom"` — flips the recipe into "server pulls inputs from the player's inventory" mode. Independent of the bucket type, so e.g. an `on_create` / `self` recipe can be magnetic. See `spacetime/server/spacetimedb/src/magnetic.rs`. |
| `output_success` | object | Cards produced when the action completes normally. Two-axis nested map: outer key picks the **place** (`"inventory"` today; future: `"hex"`, `"world"`, …), inner key picks the **owner** (`"root"` for the chain root's holder, `"actor"` for the actor's holder, `"hex"`, `"action"`). Each leaf is an array of output entities — one output card per top-level entry, `WeightedOr` entities pick one alternative. Optional. See *Output targets*. For magnetic outers, fires when an inner gets queued; for everything else, fires at action completion. |
| `output_fail` | object | **Magnetic outers only.** Same shape as `output_success`. Fires when the magnetic phase exhausts its loop budget (`duration` ticks) without ever queueing an inner. Setting this on a non-magnetic recipe is a hard build error. |
| `style` | array | Client-side rendering hints (the server ignores this entirely — pass-through field). |

## Trigger types

| Bucket / direction | Server-side `RecipeType` | Fired by | Chain shape passed to matcher |
| --- | --- | --- | --- |
| `stack` / `up` | `Stack(Up)` | `submit_inventory_stacks` | `[root, stack_up[0], stack_up[1], …]` |
| `stack` / `down` | `Stack(Down)` | `submit_inventory_stacks` | `[root, stack_down[0], stack_down[1], …]` |
| `on_create` / `self` | `OnCreate` | `insert_card_row` (every card creation, including products from completing actions) | `[new_card]` — the actor is also the root |

`stack` recipes match by sliding the actor along the chain trying every
position; the highest-weight match wins. `on_create` matches the recipe's
`root` entity directly against the new card.

## Entity grammar

An **entity** is a condition tree. Used both for slot constraints (does
this card fit?) and for product selection (which card to produce).

| Form | Meaning | Example |
| --- | --- | --- |
| `"key"` | Card with this programmatic key (e.g. `"corpus"`) | `"corpus"` |
| `"any"` | **Wildcard** — matches any card. Lowest specificity. | `"any"` |
| `"@<type>"` | **Card-type match** — any card whose `card_type` is `<type>`. The type name is resolved against [`../cards/types.json`](../cards/types.json) at registry-build time; an unknown type is a hard build error. | `"@discipline"` |
| `[name, n]` | Aspect check: `aspect[name] >= n` | `["combat", 3]` |
| `[A, B]` | AND — both must match | `[["labor", 1], "axe"]` |
| `[A, [], B]` | OR — either matches | `[["labor", 1], [], ["wood", 1]]` |
| `[A, [w1, w2], B]` | Weighted OR — pick A with weight w1 or B with weight w2. Used in `output_success` / `output_fail`; in `slots` it's treated as an unweighted OR (slots care whether a card matches, not which alternative was picked). | `["log", [4, 1], "vigor"]` |
| `[X]` | Degenerate one-element wrapper, unwraps to `X`. The most common form for single-entity slots. | `["corpus"]` → `"corpus"` |

### Disambiguation rules

- **`[s, n]` vs `[A, B]`**: if the second element is a number, it's an
  aspect check. Otherwise it's an AND.
- **`[A, M, B]` (3-tuple)**: if `M` is `[]` it's OR, if `M` is `[w1, w2]`
  (two numbers) it's WeightedOr. Anything else is a parser error.
- **Reserved string sentinels**: `"any"` is parsed as the wildcard, and
  any string starting with `@` is parsed as a card-type match. Card keys
  must therefore avoid these forms — no card with key `"any"`, no key
  starting with `@`. Today no card data conflicts.

### Aspect names

Every aspect-name string must exist in
[`../cards/aspects.json`](../cards/aspects.json). Names are translated to numeric
`AspectId`s at recipe-registry build time; an unknown name is a hard
build error and the registry refuses to load.

### Card-key strings in outputs

Bare card-key strings (`"fatigue"`, `"log"`) resolve to a
`packed_definition` via `find_packed_by_key`, which uses the
`by_key` map populated from [`../cards/id.json`](../cards/id.json) — an
O(log n) lookup with no scanning. Today card keys are globally unique;
if two types ever share a key, the lookup returns whichever was inserted
last. If you need a specific type, use the full `"type/key"` form via
the server's `find_packed` (or wait for `parse_entity` to recognize the
prefix in recipe data — currently it parses `"type/key"` strings as a
single card key, which never resolves).

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
| `"key"` (Card) | **4** |
| `[name, n]` (Aspect) | **3** |
| `"@<type>"` (Type) | **2** |
| `"any"` | **1** |

### Composite weights

| Composite | Weight when matched |
| --- | --- |
| `[A, B]` (AND) | `weight(A) + weight(B)` — both children must match; a slot with `["corpus", ["labor", 1]]` scores 4+3=7, strictly more specific than either alone. |
| `[A, [], B]` (OR) | Weight of whichever branch satisfied (first-checked branch wins on ties). |
| `[A, [w1, w2], B]` (WeightedOr) | Same as OR for slot-side weight — the `(w1, w2)` weights only matter when picking output cards at completion. |

### Worked example: card-key beats aspect

```json
[
  {
    "type": "stack",
    "up": [
      { "id": "corpus_up_aspect",
        "slots": [["corpus", 1], ["corpus"]], "reagents": [1], "duration": 10 },
      { "id": "corpus_up",
        "slots": [["corpus"], ["corpus"]], "reagents": [1],
        "output_success": { "inventory": { "root": ["fatigue"] } }, "duration": 10 }
    ]
  }
]
```

Both match a stack of two corpus cards. Tier weights:

| Recipe | hex | root | slot 1 | slot 2 | slot_weight | Total |
| --- | --- | --- | --- | --- | --- | --- |
| `corpus_up_aspect` | 0 | 0 | Aspect → 3 | Card → 4 | 7 | (0, 0, 7) |
| `corpus_up` | 0 | 0 | Card → 4 | Card → 4 | 8 | (0, 0, 8) |

`corpus_up` wins (8 > 7) — the more specific actor slot dominates. The
`_aspect` recipe acts as a fallback for stacks where the actor isn't
literally a corpus card but still has the corpus aspect (forward-looking
for cards that contribute corpus via different keys).

Same idea for `hex: "forest"` vs `hex: ["wood", 1]`: forest scores 4 in
the hex tier, wood scores 3, comparison stops at the first tier —
forest wins regardless of the rest.

### Tiebreak

If two recipes produce identical `MatchWeight`s, the matcher keeps the
**first-encountered** match (registry-declaration order across files in
`RECIPES_FILES`, then within each file's JSON array). This is mostly
relevant for catch-all "fallback" recipes — declare them later in the
file so a more specific recipe always wins on ties.

### Where this is implemented

The priority evaluation lives **on both sides of the wire** and must
produce identical results.

- **Server (Rust)** — `actions.rs::score_recipe_for_actor` and the
  `process_top_branch` / `process_bottom_branch` helpers, using
  `entity_match_weight` and `MatchWeight`. The server is the
  **authoritative** evaluator: when the client commits a stack, the
  server independently re-runs the priority calculation against the
  submitted card_ids and starts whichever action wins.
- **Client (TypeScript)** — runs the same evaluation locally as a
  **pre-filter** to decide *whether to send the stack at all*. If no
  recipe would match (or the match is one the server is already
  running), the client doesn't bother committing. This is what keeps
  inventory fiddling from spamming the server with submissions that
  would just no-op.

This dual implementation buys two independent things at once:

- **Efficiency.** The client doesn't waste a round-trip on every
  drag-drop — only when a commit would actually change server-side
  state.
- **Security.** The server doesn't blindly trust the client's view of
  "which recipe should fire." A malicious client claiming a specific
  outcome can't manufacture one; the server scores every matching
  recipe itself and picks the highest-weight winner. The client
  pre-filter is purely *advisory*.

Both sides read the same `data/recipes/data/*.json` and `data/recipes/id.json`,
so the **input** to evaluation is automatically in sync — but the
**evaluation logic** has to be kept in lockstep manually. When you
change anything about priority semantics (adding an entity form,
changing weight constants, tweaking composite rules, extending the
`hex` tier), **change both implementations in the same commit**.

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
arrangement. This is what keeps a worse arrangement from "upgrading"
itself by sliding a different card into a slot.

**The chain root is fluid and unheld.** The chain root is *not* in
`CardHold` — leaving it free is what lets multiple recipes share one,
e.g. `[attack, sword] + human` over the top branch and
`[heal, anima] + human` over the bottom running concurrently. The
matcher re-validates `recipe.root` against the current chain root on
every upgrade pass; a drifted-but-still-matching root keeps the action
running, and a root that no longer matches cancels it. This is what
lets the player swap one tree for another without restarting the
woodcutting timer.

Two practical consequences for recipe authors:

- A no-op submission (same stack, same arrangement) **does not reset
  the action timer**.
- A "better" recipe at the same actor position pre-empts a worse one
  immediately. The lex-ordered priority weights decide which is
  better.

Full mechanics, including the visible-chain walk and the
defense-in-depth check at completion, are documented in
[`spacetime/docs/recipe-upgrade.md`](../../spacetime/docs/recipe-upgrade.md).

## Duration

Two shapes:

```json
"duration": 30
```

Fixed: 30 seconds.

```json
"duration": [
  [20, [["fleeting", 4]]],
  [15, [["fleeting", 3]]],
  [10, [["fleeting", 2]]],
  5
]
```

Conditional: each entry is `[seconds, condition]`. Conditions are
evaluated against the **aspect pool** of the action's claimed cards
(sum of every claimed card's aspect contributions). The first matching
entry wins. The trailing **bare number** is the fallback if no condition
matches — required, not optional.

`Card`-form and `"@type"`-form entities don't apply to an aspect pool
and never satisfy duration conditions. `"any"` always satisfies. Use
aspect entities for meaningful duration conditions.

## Reagent indexing

Reagents are positions to consume on completion. Indexing rules:

- **`0`** — the chain root.
  - For `on_create`: the actor card itself (it's both root and actor).
    Resolves to `Action.card_id`.
  - For `stack` recipes (either direction): **currently a no-op** regardless
    of whether `root` is set. The chain root isn't held in `CardHold`
    — leaving it unheld is what lets multiple recipes share one root
    concurrently — and isn't recoverable from server state at
    completion time. List slot 1 (the actor) in `reagents` instead if
    you want to consume something. When world layers land and the
    chain root gets a server-side representation, this will be
    reconnected.
- **`1`** — slot 1 (the actor). Always resolves to `Action.card_id`.
- **`N >= 2`** — the slot N card is resolved at completion by walking
  `N - 1` steps from the actor along `micro_location` within the
  action's claimed card set. Slot 1 is the actor; slot 2 is the
  claimed card whose `micro_location == actor`; slot 3 is the claimed
  card whose `micro_location == slot2`; and so on. A missing link in
  the chain is a silent no-op for that reagent.

## Output targets

A target is a `(place, owner)` pair, written in JSON as nested keys
under `output_success` (fires on normal completion) or `output_fail`
(fires only on magnetic-outer timeout):

```json
"output_success": {
  "<place>": {
    "<owner>": [/* entities */]
  }
}
```

### Place — *what kind* of destination

| Place | Resolves to |
| --- | --- |
| `"inventory"` | A player's inventory panel (layer 1, scoped by `macro_zone == panel_player_id`). |

Future places (loose world spot, hex tile transmutation, …) plug in
here as one extra entry per kind, without renaming the existing keys.

### Owner — *which referent* of the action

| Owner | Resolves to |
| --- | --- |
| `"root"` | The chain root's holder. Ideally the player whose panel the chain root sits in; today the chain root isn't tracked server-side at completion, so this falls back to the actor's holder. Fine for the inventory POC where every claimed card is in the same player's panel. Will diverge from `"actor"` once world layers land. |
| `"actor"` | The actor's holder (`actor.macro_zone`). |
| `"hex"` | The hex card's owner (`hex.owner_id`). At completion, the server walks from the actor toward the chain root via `micro_location`; if the chain ends in a rect-on-hex root (`stacked_state == 3`), the hex pointed to is resolved (from a `Card` row first, falling back to a `Zone` packed cell). Falls back to the actor's panel if the chain isn't on a hex, the hex is unowned (`owner_id == 0`), or the hex resolved from a `Zone` cell (which carries no per-cell ownership). |

Each entry in a target's array produces **one output card**. To produce
multiple cards of the same type, list it multiple times. To produce one
of several alternatives, use a `WeightedOr`.

## Stable recipe IDs

Recipe `id` strings (`"woodcutting"` etc.) are mapped to stable integer
IDs in [`id.json`](id.json):

```json
{
  "corpus_up_aspect": 1,
  "corpus_down_aspect": 2,
  "corpus_up": 3,
  ...
}
```

These integers are what `Action.recipe` stores on the wire — they
**never change** once assigned. Adding a new recipe gets a new integer;
removing a recipe leaves its integer reserved as a tombstone so it can't
be recycled.

`id.json` is generated by [`../gen-ids.py`](../gen-ids.py). Run it
before `spacetime build` whenever you add or remove recipes — the
registry-build step refuses to load a recipe whose `id` isn't in
`id.json` and errors with `not found in recipes/id.json — run gen-ids.py`.

## Worked examples (from `data/01.json`)

The examples below show just the recipe record — in `data/01.json` each one
sits inside a `{ "type": "stack", "up": [ … ] }` (or analogous) bucket.

### `corpus_up` — the reference up-stack recipe

```json
{
  "id": "corpus_up",
  "slots": [["corpus"], ["corpus"]],
  "reagents": [1],
  "output_success": { "inventory": { "root": ["fatigue"] } },
  "duration": 10
}
```

Two corpus cards stacked → 10 second action → produces a fatigue card to
the root's panel, consumes the actor (slot 1, the lower corpus). The
upper corpus survives.

### `corpus_up_aspect` — aspect-based fallback

```json
{
  "id": "corpus_up_aspect",
  "slots": [["corpus", 1], ["corpus"]],
  "reagents": [1],
  "duration": 10
}
```

Same shape but slot 1 is an aspect check (`corpus >= 1`) rather than a
card-key match. Loses to `corpus_up` on a stack of two corpus cards (4+4
> 3+4), but would win if the actor card weren't keyed `"corpus"` while
still carrying the corpus aspect — currently no such card exists.

### `woodcutting` — OR-aspect slot, weighted product

```json
{
  "id": "woodcutting",
  "slots": [
    [["labor", 1], [], ["wood", 1]],
    ["corpus"]
  ],
  "reagents": [1],
  "output_success": {
    "inventory": {
      "root":  ["fatigue"],
      "actor": [["log", [4, 1], "vigor"]]
    }
  },
  "duration": 30
}
```

Slot 1 (the actor): a card with either labor ≥ 1 OR wood ≥ 1 aspect.
Slot 2: a corpus. 30 second duration. Produces a fatigue card to the
root's panel and (4:1 weighted) either a log or a vigor to the actor's
panel. Consumes slot 1 (the actor card).

### `fatigue` — on_create cascade

```json
{
  "id": "fatigue",
  "root": ["fatigue"],
  "reagents": [0],
  "output_success": { "inventory": { "actor": ["corpus"] } },
  "duration": 30
}
```

Lives inside a `{ "type": "on_create", "self": [ … ] }` bucket.
Triggers when a fatigue card is created (e.g. the product of
`corpus_up`). 30 seconds later, produces a corpus to the same panel and
consumes the fatigue itself. Closes the
`corpus_up → fatigue → corpus` loop.

### `fleeting` — conditional duration, no outputs

```json
{
  "id": "fleeting",
  "root": [["fleeting", 1]],
  "reagents": [0],
  "duration": [
    [20, [["fleeting", 4]]],
    [15, [["fleeting", 3]]],
    [10, [["fleeting", 2]]],
    5
  ]
}
```

Triggers on any new card with the `fleeting` aspect. Duration scales
with the fleeting value: 20s if ≥4, 15s if ≥3, 10s if ≥2, fallback 5s.
Consumes the card on completion (no `output_success` field = the card
just expires).

## Conventions

- **`id`** is `lowercase_snake_case`, descriptive of the recipe's
  effect.
- **Slot wrapping**: every slot uses the `[entity]` form even for a
  single string entity (`["corpus"]` not just `"corpus"`). Consistent
  visual shape.
- **Conditional duration ordering**: write cases from highest threshold
  to lowest, with the fallback last. The matcher walks them in order
  and takes the first that satisfies.
- **`reagents: [1]` is the most common shape** — most recipes consume
  the actor and leave the rest of the stack alone.
- **Catch-all recipes go later in the file** — when two recipes produce
  identical `MatchWeight`s, declaration order is the tiebreak.
  Specific-then-general ordering keeps the more specific recipe
  preferred.

## Pitfalls

- **Recipe `id` not in `recipes/id.json`**: hard build error. Run
  `gen-ids.py` after adding a new recipe.
- **Duplicate `id` across files**: hard build error.
- **Unknown aspect name in any entity**: hard build error at registry
  load.
- **Unknown card type in `"@<type>"` entity**: hard build error.
- **Missing fallback in conditional duration**: build error. The
  trailing bare number is required.
- **Reagent 0 in a `stack` recipe**: silent no-op (the chain root
  isn't held by the action). Use `reagents: [1]` to consume the actor
  instead. Reconnects when world layers land and the chain root gets
  a server-side representation.
- **Reagent N ≥ 2**: silently skipped. Not yet supported.
- **Reserved-string collisions**: a card whose key is `"any"` would
  shadow the wildcard; a card whose key starts with `@` would parse as
  a type match. Today no card data trips this; if you ever add one,
  rename the card.
- **`hex` field requires `stacked_state == 3` on chain[0]** — the
  matcher reads the chain root's micro_zone and only resolves the hex
  when the root is rect-on-hex. Inventory chains
  (`stacked_state == 0` always for layer-1 cards) never satisfy a hex
  precondition; the field is meaningful only for world stacks where
  rectangles are anchored on hex cards. The hex itself can be a full
  `Card` row or a packed `Zone` cell — the matcher tries `cards`
  first, then falls back to `zones`.

## Loader

[`definitions.rs`](../../spacetime/server/spacetimedb/src/definitions.rs)
— `build_recipes`, `parse_entity`, `parse_duration`. Recipe parsing
pulls `type_ids` from the cards registry to resolve `"@<type>"`
strings. New files MUST be added to the `RECIPES_FILES` const tuple.

## Adding a new recipe

1. Pick the file (or add a new one).
2. Append the recipe record. (Order within a file affects priority
   tiebreaks — declare specific recipes before catch-all ones.)
3. Confirm every aspect name in entities exists in `aspects.json`.
4. Confirm every card-type name in `"@<type>"` entities exists in
   `cards/types.json`.
5. Confirm every card key in `output_success` / `output_fail` exists in `../cards/data/*.json`.
6. Run `python data/gen-ids.py` to assign the new recipe a stable ID
   in `recipes/id.json`.
7. If a new file: add it to `RECIPES_FILES` in `definitions.rs`.
