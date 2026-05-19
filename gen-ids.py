#!/usr/bin/env python3
"""
gen-ids.py — Generate stable ID mappings for recipes, cards, starter packs, blueprints, and textures.

Produces:
  recipes/id.json        { "<recipe-id>": <stable-int>, ... }
  cards/id.json          { "<card_type>": { "<key>": <definition_id>, ... } }
  starter_packs/id.json  { "<soul>": { "<pack_id>": <stable-int>, ... } }
  blueprints/id.json     { "<blueprint_key>": <stable-int>, ... }
  textures/id.json       { "<card_type>": { "<key>": <texture_id>, ... } }

Reads recipe / card / starter-pack / blueprint / texture definition files
from `<root>/data/**/*.json` under each subsystem. Sibling metadata
(types.json, aspects.json, flags.json, traits.json, the id.json files
themselves) is ignored.

Recipe IDs share one flat namespace under the tape-form grammar (see
`pixijs/src/content/src/recipe_tape.rs`). Each recipe file is shaped
`{ "<recipe-id>": { "input": [...], "output": [...] }, ... }`.
Duplicate recipe ids across any file are an error.

Card definition_ids are u12 (1–4095) scoped per card_type; 0 is reserved
as sentinel. The server combines a card's definition_id with its
`card_type` at build time to produce the packed_definition wire format.
(The `card_category` dimension was retired — see
docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md.)

Starter pack IDs are scoped per soul: two souls can share the same pack id
(e.g. both have a `"default"`), but `(soul, pack_id)` pairs must be unique.

Texture IDs are u16 (1–65535) scoped per card_type — textures live in
the same `<type>.<key>` namespace that cards do, but the key set is
independent (a `tile/wood` texture has no relation to a `tile/wood`
card, if one ever existed). Two types can each have a `"wood"`
texture with id 1.

Modes:
  default      Rewrite both id.json files from scratch. IDs are reassigned
               on every run in source-discovery order — fine while wire
               compatibility is not yet a constraint.
  --skip-known Preserve IDs already present in id.json (the historical
               "stable IDs, append-only, tombstone removed entries"
               behavior). Use this once IDs are baked into save data,
               persisted action rows, or anything else on the wire.

Run before `spacetime build`.
"""

import argparse
import json
import sys
from pathlib import Path


def load_json(path: Path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def next_id(existing: dict) -> int:
    return max(existing.values(), default=0) + 1


# ── Recipes ────────────────────────────────────────────────────────────────────


def gen_recipe_ids(data_dir: Path, skip_known: bool) -> bool:
    recipes_dir = data_dir / "recipes"
    defs_dir = recipes_dir / "data"
    id_path = recipes_dir / "id.json"

    if skip_known:
        raw = load_json(id_path) if id_path.exists() else {}
        # Detect old nested-by-(type,category) format and discard it —
        # tape-form recipes share one flat id namespace.
        if raw and any(isinstance(v, dict) for v in raw.values()):
            print("  NOTE: recipes/id.json is in old nested format — resetting to flat format")
            raw = {}
        existing: dict = raw
    else:
        # Fresh-rebuild mode: ignore the existing file entirely. Every
        # entry is treated as new; no tombstones are preserved.
        existing = {}

    all_ids: list[str] = []
    seen: dict[str, str] = {}  # rid -> first file that declared it
    errors: list[str] = []

    for recipe_file in sorted(defs_dir.rglob("*.json")):
        rel = recipe_file.relative_to(defs_dir).as_posix()
        try:
            root = load_json(recipe_file)
        except json.JSONDecodeError as e:
            errors.append(f"{rel}: JSON parse error: {e}")
            continue
        # Tape-form shape: { "<recipe-id>": { "input": [...], "output": [...] }, ... }
        if not isinstance(root, dict):
            errors.append(f"{rel}: top-level must be an object keyed by recipe id")
            continue
        for rid, recipe_obj in root.items():
            # Skip JSON-doc convention keys (`_comment`, etc.).
            if rid.startswith("_"):
                continue
            if not isinstance(recipe_obj, dict):
                errors.append(f"{rel}: recipe {rid!r}: value must be an object with `input` and `output` arrays")
                continue
            if "input" not in recipe_obj or "output" not in recipe_obj:
                errors.append(f"{rel}: recipe {rid!r}: missing `input` and/or `output` array")
                continue
            if rid in seen:
                errors.append(f"Duplicate recipe id '{rid}' in {rel} (first seen in {seen[rid]})")
            else:
                seen[rid] = rel
                all_ids.append(rid)

    if errors:
        for e in errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        return False

    result: dict = dict(existing) if skip_known else {}
    nid = next_id(result) if result else 1
    n_added = 0

    for rid in all_ids:
        if rid not in result:
            result[rid] = nid
            if skip_known:
                print(f"    + {rid} = {nid}")
            n_added += 1
            nid += 1

    n_removed = 0
    if skip_known:
        active = set(all_ids)
        removed = [k for k in existing if k not in active]
        for r in removed:
            print(f"  WARNING: '{r}' (id={existing[r]}) no longer in source — ID reserved")
        n_removed = len(removed)

    result_sorted = dict(sorted(result.items(), key=lambda kv: kv[1]))

    with open(id_path, "w", encoding="utf-8") as f:
        json.dump(result_sorted, f, indent=2)
        f.write("\n")

    n_active = len(all_ids)
    print(f"  {n_active} active, {n_added} new, {n_removed} removed")
    return True


# ── Cards ──────────────────────────────────────────────────────────────────────

MAX_DEFINITION_ID = 0xFFF  # u12; 0 is sentinel


def gen_card_ids(data_dir: Path, skip_known: bool) -> bool:
    cards_dir = data_dir / "cards"
    defs_dir = cards_dir / "data"
    id_path = cards_dir / "id.json"

    if skip_known:
        existing: dict = load_json(id_path) if id_path.exists() else {}
    else:
        # Fresh-rebuild mode: ignore the existing file entirely. Every
        # key is treated as new; no tombstones are preserved.
        existing = {}

    # Collect cards grouped by type, checking for duplicate keys
    # globally. The `category` middle dimension was retired (see
    # docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md) — card defs now nest
    # `{ <type>: { <key>: { ... } } }`.
    by_type: dict[str, list[str]] = {}
    all_keys: dict[str, str] = {}  # key -> type for duplicate check
    errors: list[str] = []

    for card_file in sorted(defs_dir.rglob("*.json")):
        rel = card_file.relative_to(defs_dir).as_posix()
        try:
            root = load_json(card_file)
        except json.JSONDecodeError as e:
            errors.append(f"{rel}: JSON parse error: {e}")
            continue
        if not isinstance(root, dict):
            errors.append(f"{rel}: top-level must be an object keyed by card_type")
            continue
        for type_name, cards_obj in root.items():
            # Skip JSON-doc convention keys (`_comment`, etc.) at the
            # type level — same rule as the recipes-file parser above.
            if type_name.startswith("_"):
                continue
            if not isinstance(cards_obj, dict):
                errors.append(f"{rel}: type {type_name!r}: value not an object")
                continue
            for key in cards_obj:
                # Skip JSON-doc convention keys at the card level
                # (sibling to real card keys within a type).
                if key.startswith("_"):
                    continue
                if key in all_keys:
                    prev = all_keys[key]
                    errors.append(
                        f"Duplicate card key '{key}' in {rel}"
                        f" (first seen under type '{prev}')"
                    )
                else:
                    all_keys[key] = type_name
                    by_type.setdefault(type_name, []).append(key)

    if errors:
        for e in errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        return False

    result: dict = {}
    n_active = n_added = n_removed = 0

    for type_name in sorted(by_type):
        keys = by_type[type_name]
        type_existing = existing.get(type_name, {})
        type_result = dict(type_existing)
        nid = next_id(type_result)

        for key in keys:
            if key not in type_result:
                if nid > MAX_DEFINITION_ID:
                    print(
                        f"  ERROR: '{type_name}' definition_id {nid} exceeds u12 max ({MAX_DEFINITION_ID})",
                        file=sys.stderr,
                    )
                    return False
                type_result[key] = nid
                if skip_known:
                    print(f"    + {type_name}/{key} = {nid}")
                n_added += 1
                nid += 1

        if skip_known:
            removed = [k for k in type_existing if k not in set(keys)]
            for r in removed:
                print(f"  WARNING: '{type_name}/{r}' (id={type_existing[r]}) no longer in source — ID reserved")
            n_removed += len(removed)
        n_active += len(keys)

        result[type_name] = dict(
            sorted(type_result.items(), key=lambda kv: kv[1])
        )

    if skip_known:
        # Preserve tombstoned types entirely.
        for type_name, entries in existing.items():
            if type_name not in result:
                print(f"  WARNING: '{type_name}' no longer in source — IDs reserved")
                result[type_name] = entries

    with open(id_path, "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
        f.write("\n")

    print(f"  {n_active} active, {n_added} new, {n_removed} removed")
    return True


# ── Starter packs ──────────────────────────────────────────────────────────────


def gen_starter_pack_ids(data_dir: Path, skip_known: bool) -> bool:
    packs_dir = data_dir / "starter_packs"
    defs_dir = packs_dir / "data"
    id_path = packs_dir / "id.json"

    if not defs_dir.exists():
        # No starter-pack data tree — nothing to generate. Skip silently.
        print("  (no starter_packs/data directory, skipping)")
        return True

    if skip_known:
        existing: dict = load_json(id_path) if id_path.exists() else {}
    else:
        existing = {}

    # Collect packs grouped by soul. (soul, pack_id) pairs must be unique;
    # two souls may share a pack_id like "default".
    by_soul: dict[str, list[str]] = {}
    all_pairs: dict[tuple[str, str], str] = {}  # (soul, pack_id) -> filename
    errors: list[str] = []

    for pack_file in sorted(defs_dir.rglob("*.json")):
        rel = pack_file.relative_to(defs_dir).as_posix()
        try:
            top = load_json(pack_file)
        except json.JSONDecodeError as e:
            errors.append(f"{rel}: JSON parse error: {e}")
            continue
        # Accept either a single { soul: { pack_id: {...} } } object or a
        # top-level array of such objects — mirrors the card / recipe
        # loaders' shape flexibility.
        if isinstance(top, dict):
            entries = [top]
        elif isinstance(top, list):
            entries = top
        else:
            errors.append(f"{rel}: top-level must be an object or array of objects")
            continue

        for entry in entries:
            if not isinstance(entry, dict):
                errors.append(f"{rel}: entry not an object")
                continue
            for soul_key, packs_obj in entry.items():
                if not isinstance(packs_obj, dict):
                    errors.append(f"{rel}: soul {soul_key!r}: pack map is not an object")
                    continue
                for pack_id, body in packs_obj.items():
                    # `blueprints` is a per-soul list of starter blueprint
                    # keys (the set of blueprints granted on character
                    # creation), not a redeemable pack — skip the id
                    # assignment but still validate the shape so a
                    # malformed entry surfaces here rather than at
                    # registry-build time. The Rust loader resolves the
                    # individual blueprint keys.
                    if pack_id == "blueprints":
                        if not isinstance(body, list):
                            errors.append(
                                f"{rel}: soul {soul_key!r}: `blueprints` must be a JSON array of blueprint keys"
                            )
                        continue
                    pair = (soul_key, pack_id)
                    if pair in all_pairs:
                        errors.append(
                            f"Duplicate starter pack '{soul_key}/{pack_id}' in {rel}"
                            f" (first seen in {all_pairs[pair]})"
                        )
                    else:
                        all_pairs[pair] = rel
                        by_soul.setdefault(soul_key, []).append(pack_id)

    if errors:
        for e in errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        return False

    result: dict = {}
    n_active = n_added = n_removed = 0

    for soul_key in sorted(by_soul):
        pack_ids = by_soul[soul_key]
        soul_existing = existing.get(soul_key, {})
        soul_result = dict(soul_existing)
        nid = next_id(soul_result)

        for pack_id in pack_ids:
            if pack_id not in soul_result:
                soul_result[pack_id] = nid
                if skip_known:
                    print(f"    + {soul_key}/{pack_id} = {nid}")
                n_added += 1
                nid += 1

        if skip_known:
            removed = [k for k in soul_existing if k not in set(pack_ids)]
            for r in removed:
                print(f"  WARNING: '{soul_key}/{r}' (id={soul_existing[r]}) no longer in source — ID reserved")
            n_removed += len(removed)
        n_active += len(pack_ids)

        result[soul_key] = dict(
            sorted(soul_result.items(), key=lambda kv: kv[1])
        )

    if skip_known:
        # Preserve tombstoned souls entirely.
        for soul_key, entries in existing.items():
            if soul_key not in result:
                print(f"  WARNING: '{soul_key}' no longer in source — IDs reserved")
                result[soul_key] = entries

    with open(id_path, "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
        f.write("\n")

    print(f"  {n_active} active, {n_added} new, {n_removed} removed")
    return True


# ── Blueprints ─────────────────────────────────────────────────────────────────


def gen_blueprint_ids(data_dir: Path, skip_known: bool) -> bool:
    """Blueprints share one flat id namespace, same shape as recipes —
    `{ "<blueprint_key>": <stable-int>, ... }`. The body schema is
    intentionally lax: this pass only collects the top-level keys for
    id assignment. The Rust loader validates body fields (`blueprint`
    card key + `card` output key) at registry-build time."""
    blueprints_dir = data_dir / "blueprints"
    defs_dir = blueprints_dir / "data"
    id_path = blueprints_dir / "id.json"

    if not defs_dir.exists():
        # No blueprint data tree — nothing to generate. Skip silently.
        print("  (no blueprints/data directory, skipping)")
        return True

    if skip_known:
        existing: dict = load_json(id_path) if id_path.exists() else {}
    else:
        existing = {}

    all_keys: list[str] = []
    seen: dict[str, str] = {}  # key -> first file that declared it
    errors: list[str] = []

    for bp_file in sorted(defs_dir.rglob("*.json")):
        rel = bp_file.relative_to(defs_dir).as_posix()
        try:
            root = load_json(bp_file)
        except json.JSONDecodeError as e:
            errors.append(f"{rel}: JSON parse error: {e}")
            continue
        if not isinstance(root, dict):
            errors.append(f"{rel}: top-level must be an object keyed by blueprint key")
            continue
        for key, body in root.items():
            # Skip JSON-doc convention keys (`_comment`, etc.).
            if key.startswith("_"):
                continue
            if not isinstance(body, dict):
                errors.append(f"{rel}: blueprint {key!r}: body must be an object")
                continue
            if key in seen:
                errors.append(
                    f"Duplicate blueprint key '{key}' in {rel}"
                    f" (first seen in {seen[key]})"
                )
            else:
                seen[key] = rel
                all_keys.append(key)

    if errors:
        for e in errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        return False

    result: dict = dict(existing) if skip_known else {}
    nid = next_id(result) if result else 1
    n_added = 0

    for key in all_keys:
        if key not in result:
            result[key] = nid
            if skip_known:
                print(f"    + {key} = {nid}")
            n_added += 1
            nid += 1

    n_removed = 0
    if skip_known:
        active = set(all_keys)
        removed = [k for k in existing if k not in active]
        for r in removed:
            print(f"  WARNING: '{r}' (id={existing[r]}) no longer in source — ID reserved")
        n_removed = len(removed)

    result_sorted = dict(sorted(result.items(), key=lambda kv: kv[1]))

    with open(id_path, "w", encoding="utf-8") as f:
        json.dump(result_sorted, f, indent=2)
        f.write("\n")

    n_active = len(all_keys)
    print(f"  {n_active} active, {n_added} new, {n_removed} removed")
    return True


# ── Textures ───────────────────────────────────────────────────────────────────

MAX_TEXTURE_ID = 65535  # u16; 0 is sentinel


def gen_texture_ids(data_dir: Path, skip_known: bool) -> bool:
    textures_dir = data_dir / "textures"
    defs_dir = textures_dir / "data"
    id_path = textures_dir / "id.json"

    if not defs_dir.exists():
        # No texture data tree — nothing to generate. Skip silently.
        print("  (no textures/data directory, skipping)")
        return True

    if skip_known:
        existing: dict = load_json(id_path) if id_path.exists() else {}
    else:
        # Fresh-rebuild mode: ignore the existing file entirely.
        existing = {}

    # Collect textures grouped by card_type. Texture keys are scoped
    # per type — `tile/wood` and `requisite/wood` are independent —
    # so no global uniqueness check is needed. (`category` middle
    # level retired with the cards-side retire — see
    # docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md.)
    by_type: dict[str, list[str]] = {}
    errors: list[str] = []

    for tex_file in sorted(defs_dir.rglob("*.json")):
        rel = tex_file.relative_to(defs_dir).as_posix()
        try:
            root = load_json(tex_file)
        except json.JSONDecodeError as e:
            errors.append(f"{rel}: JSON parse error: {e}")
            continue
        if not isinstance(root, dict):
            errors.append(f"{rel}: top-level must be an object keyed by card_type")
            continue
        for type_name, textures_obj in root.items():
            # Skip JSON-doc convention keys (`_comment`, etc.).
            if type_name.startswith("_"):
                continue
            if not isinstance(textures_obj, dict):
                errors.append(f"{rel}: type {type_name!r}: value not an object")
                continue
            for key in textures_obj:
                if key.startswith("_"):
                    continue
                type_keys = by_type.setdefault(type_name, [])
                if key in type_keys:
                    errors.append(
                        f"Duplicate texture key '{type_name}/{key}' in {rel}"
                    )
                else:
                    type_keys.append(key)

    if errors:
        for e in errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        return False

    result: dict = {}
    n_active = n_added = n_removed = 0

    for type_name in sorted(by_type):
        keys = by_type[type_name]
        type_existing = existing.get(type_name, {})
        type_result = dict(type_existing)
        nid = next_id(type_result)

        for key in keys:
            if key not in type_result:
                if nid > MAX_TEXTURE_ID:
                    print(
                        f"  ERROR: '{type_name}' texture_id {nid} exceeds u16 max ({MAX_TEXTURE_ID})",
                        file=sys.stderr,
                    )
                    return False
                type_result[key] = nid
                if skip_known:
                    print(f"    + {type_name}/{key} = {nid}")
                n_added += 1
                nid += 1

        if skip_known:
            removed = [k for k in type_existing if k not in set(keys)]
            for r in removed:
                print(f"  WARNING: '{type_name}/{r}' (id={type_existing[r]}) no longer in source — ID reserved")
            n_removed += len(removed)
        n_active += len(keys)

        result[type_name] = dict(
            sorted(type_result.items(), key=lambda kv: kv[1])
        )

    if skip_known:
        # Preserve tombstoned types entirely.
        for type_name, entries in existing.items():
            if type_name not in result:
                print(f"  WARNING: '{type_name}' no longer in source — IDs reserved")
                result[type_name] = entries

    with open(id_path, "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
        f.write("\n")

    print(f"  {n_active} active, {n_added} new, {n_removed} removed")
    return True


# ── Locales ────────────────────────────────────────────────────────────────────
#
# Each domain (cards / recipes / starter_packs / …) keeps its
# translations under `locales/<domain>/<lang>.json` with the same
# nested path structure as that domain's `id.json`. `sync_locales`
# walks every source-data file in the domain, extracts the set of
# (path-segment) keys that exist, then makes sure `en.json` has at
# minimum a `{"label": "<last-segment>"}` entry for each.
#
# Discipline:
#
# - **Never overwrite** an existing label or description — translation
#   work is preserved across runs in both fresh-rebuild and skip-known
#   modes.
# - **Stub the minimum**: only `label` (= the bare key) is added. Authors
#   fill in `description`, `success`, etc. by hand as recipes/cards
#   mature.
# - **Warn but don't delete** orphan entries (entry in `en.json` with no
#   matching source-data key). Translators may want to keep the row
#   around through a rename / re-categorise.


def walk_card_data_paths(data_dir: Path) -> set[str]:
    """Dotted paths for every card declared under `cards/data/`.
    Shape: `<card_type>.<key>` (category was retired — see
    docs/CATEGORY_RETIRE_AND_TILE_EXPAND.md)."""
    return _walk_nested_paths(data_dir / "cards" / "data", depth=2)


def walk_recipe_data_paths(data_dir: Path) -> set[str]:
    """Dotted paths for every recipe declared under `recipes/data/`.
    Shape: `<recipe-id>` (tape-form recipes share one flat namespace)."""
    return _walk_nested_paths(data_dir / "recipes" / "data", depth=1)


def _walk_nested_paths(defs_dir: Path, depth: int) -> set[str]:
    """Walk `defs_dir/**/*.json` and return every dotted key path
    `<seg1>.<seg2>...<segN>` reached at exactly `depth` levels of
    nested object keys. Non-object intermediates are silently skipped;
    parse errors are silently skipped too (the corresponding id-gen
    pass will surface them with a real error)."""
    out: set[str] = set()
    if not defs_dir.exists():
        return out
    for file in sorted(defs_dir.rglob("*.json")):
        try:
            root = load_json(file)
        except json.JSONDecodeError:
            continue
        if not isinstance(root, dict):
            continue
        _collect_paths_at_depth(root, depth, [], out)
    return out


def _collect_paths_at_depth(
    node: dict,
    remaining: int,
    so_far: list[str],
    out: set[str],
) -> None:
    if remaining == 0:
        out.add(".".join(so_far))
        return
    for k, v in node.items():
        if not isinstance(v, dict):
            continue
        so_far.append(k)
        _collect_paths_at_depth(v, remaining - 1, so_far, out)
        so_far.pop()


def _collect_locale_paths(
    node: dict,
    so_far: list[str],
    out: set[str],
    warnings: list[str] | None = None,
) -> None:
    """Walk a locale file and emit a dotted path for every leaf
    (an object with a `label` field).

    If a node has both a `label` field AND child objects that
    themselves look like leaves (have their own `label` field), it's
    a malformed bucket masquerading as a leaf — almost always the
    result of a botched schema migration. The walker treats it as a
    bucket (recurses into children) and appends a warning if a
    `warnings` list is provided, so `sync_locales` can surface the
    problem to the operator instead of silently corrupting the
    registry."""
    has_label = "label" in node and isinstance(node.get("label"), str)
    child_leaves = [
        k for k, v in node.items()
        if isinstance(v, dict) and isinstance(v.get("label"), str)
    ]
    if has_label and child_leaves:
        if warnings is not None:
            path = ".".join(so_far) or "<root>"
            warnings.append(
                f"node {path!r} has both a `label` field "
                f"({node['label']!r}) AND child leaves "
                f"({child_leaves}); treating as bucket — remove the "
                f"stray `label` to clear this warning"
            )
        # fall through to the bucket case below
    elif has_label:
        out.add(".".join(so_far))
        return
    for k, v in node.items():
        if not isinstance(v, dict):
            continue
        so_far.append(k)
        _collect_locale_paths(v, so_far, out, warnings)
        so_far.pop()


def _ensure_locale_path(root: dict, path: list[str], leaf: dict) -> None:
    """Walk `root` along `path[:-1]`, creating empty intermediate
    objects as needed, then insert `leaf` at `path[-1]`. Does NOT
    overwrite an existing entry."""
    cur = root
    for seg in path[:-1]:
        nxt = cur.get(seg)
        if not isinstance(nxt, dict):
            nxt = {}
            cur[seg] = nxt
        cur = nxt
    last = path[-1]
    if last not in cur:
        cur[last] = leaf


def _remove_locale_path(root: dict, path: list[str]) -> None:
    """Remove the leaf at `path` from `root`, pruning any intermediate
    dicts that become empty as a result."""
    if not path:
        return
    parents: list[tuple[dict, str]] = []
    cur = root
    for seg in path[:-1]:
        nxt = cur.get(seg)
        if not isinstance(nxt, dict):
            return
        parents.append((cur, seg))
        cur = nxt
    last = path[-1]
    if last not in cur:
        return
    del cur[last]
    for parent, key in reversed(parents):
        if isinstance(parent.get(key), dict) and not parent[key]:
            del parent[key]
        else:
            break


def sync_locales(data_dir: Path, verbose: bool = False, purge_orphans: bool = False) -> bool:
    """For each domain that has an `en.json` candidate path, ensure
    `en.json` carries at minimum a label entry for every key declared
    in the corresponding `data/` tree. Returns `True` on success.

    **Retained orphans.** By default, locale entries with no matching
    source key are kept in place — translation work survives a
    delete/re-add round trip, so re-introducing a card later picks up
    its old label automatically. Pass `purge_orphans=True` (``--purge-orphans``)
    to delete them instead.
    """
    locales_dir = data_dir / "locales"
    locales_dir.mkdir(exist_ok=True)

    domains = [
        ("cards",   walk_card_data_paths(data_dir)),
        ("recipes", walk_recipe_data_paths(data_dir)),
    ]

    ok = True
    for domain, expected_paths in domains:
        en_path = locales_dir / domain / "en.json"
        en_path.parent.mkdir(exist_ok=True)
        if en_path.exists():
            try:
                root = load_json(en_path)
            except json.JSONDecodeError as e:
                print(f"  ERROR: {en_path}: parse failed: {e}", file=sys.stderr)
                ok = False
                continue
            if not isinstance(root, dict):
                print(
                    f"  ERROR: {en_path}: top-level must be an object",
                    file=sys.stderr,
                )
                ok = False
                continue
        else:
            root = {}

        existing_paths: set[str] = set()
        locale_warnings: list[str] = []
        _collect_locale_paths(root, [], existing_paths, locale_warnings)
        for w in locale_warnings:
            print(f"  WARNING: {domain}/en.json: {w}", file=sys.stderr)

        added = 0
        for path_str in sorted(expected_paths):
            if path_str in existing_paths:
                continue
            segments = path_str.split(".")
            _ensure_locale_path(
                root,
                segments,
                {"label": segments[-1]},
            )
            added += 1

        orphaned = sorted(existing_paths - expected_paths)
        if purge_orphans:
            for orphan in orphaned:
                _remove_locale_path(root, orphan.split("."))
                if verbose:
                    print(f"    (orphan purged) {domain}/en.json: {orphan}")
        else:
            if verbose:
                for orphan in orphaned:
                    print(f"    (orphan retained) {domain}/en.json: {orphan}")

        with open(en_path, "w", encoding="utf-8") as f:
            json.dump(root, f, indent=2, ensure_ascii=False)
            f.write("\n")

        active = len(expected_paths)
        n_orphans = len(orphaned)
        if purge_orphans:
            print(f"  {domain}: {active} active, {added} stubbed, {n_orphans} purged")
        else:
            print(f"  {domain}: {active} active, {added} stubbed, {n_orphans} orphan")

    return ok


# ── Entry point ────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Generate recipes/id.json, cards/id.json, and starter_packs/id.json from the JSON catalogs.",
    )
    parser.add_argument(
        "--skip-known",
        action="store_true",
        help="Preserve IDs already present in id.json (stable-ID mode with tombstones). "
             "Default rewrites both files from scratch.",
    )
    parser.add_argument(
        "--purge-orphans",
        action="store_true",
        help="Delete locale entries in en.json that have no matching source key. "
             "Default retains orphan entries so translation work survives renames.",
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Print each orphan locale path (retained or purged). Default prints "
             "only the count per domain.",
    )
    args = parser.parse_args()

    script_dir = Path(__file__).parent.resolve()
    data_dir = (script_dir).resolve()

    if not data_dir.exists():
        print(f"ERROR: data directory not found: {data_dir}", file=sys.stderr)
        sys.exit(1)

    mode = "skip-known (preserve existing IDs)" if args.skip_known else "fresh rebuild"
    print(f"Mode: {mode}")

    ok = True

    print("Recipes:")
    if not gen_recipe_ids(data_dir, args.skip_known):
        ok = False

    print("Cards:")
    if not gen_card_ids(data_dir, args.skip_known):
        ok = False

    print("Starter packs:")
    if not gen_starter_pack_ids(data_dir, args.skip_known):
        ok = False

    print("Blueprints:")
    if not gen_blueprint_ids(data_dir, args.skip_known):
        ok = False

    print("Textures:")
    if not gen_texture_ids(data_dir, args.skip_known):
        ok = False

    print("Locales:")
    if not sync_locales(data_dir, verbose=args.verbose, purge_orphans=args.purge_orphans):
        ok = False

    if not ok:
        sys.exit(1)

    print("Done.")


if __name__ == "__main__":
    main()
