#!/usr/bin/env python3
"""
gen-ids.py — Generate stable ID mappings for recipes, cards, and starter packs.

Produces:
  recipes/id.json        { "<type>": { "<category>": { "<recipe-id>": <stable-int>, ... } } }
  cards/id.json          { "<card_type>": { "<category>": { "<key>": <definition_id>, ... } } }
  starter_packs/id.json  { "<soul>": { "<pack_id>": <stable-int>, ... } }

Reads recipe / card / starter-pack definition files from
`<root>/data/**/*.json` under each subsystem. Sibling metadata
(types.json, aspects.json, flags.json, traits.json, the id.json files
themselves) is ignored.

Recipe IDs are scoped per (type, category): stack/up, stack/down, on_create/self each
have their own counter starting at 1. Duplicate recipe ids across any type/category
are an error.

Card definition_ids are u8 (1–255) scoped per (card_type, category); 0 is reserved as
sentinel. The server combines a card's definition_id with its bucket's type
and category at build time to produce the packed_definition wire format.

Starter pack IDs are scoped per soul: two souls can share the same pack id
(e.g. both have a `"default"`), but `(soul, pack_id)` pairs must be unique.

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
        # Detect old flat format { "recipe_id": int } and discard it.
        if raw and any(isinstance(v, int) for v in raw.values()):
            print("  NOTE: recipes/id.json is in old flat format — resetting to nested format")
            raw = {}
        existing: dict = raw
    else:
        # Fresh-rebuild mode: ignore the existing file entirely. Every
        # entry is treated as new; no tombstones are preserved.
        existing = {}

    by_type_cat: dict[tuple[str, str], list[str]] = {}
    all_ids: dict[str, tuple[str, str]] = {}  # id -> (type, category) for duplicate check
    errors: list[str] = []

    for recipe_file in sorted(defs_dir.rglob("*.json")):
        rel = recipe_file.relative_to(defs_dir).as_posix()
        try:
            root = load_json(recipe_file)
        except json.JSONDecodeError as e:
            errors.append(f"{rel}: JSON parse error: {e}")
            continue
        # Nested shape:
        #   { "<type>": { "<category>": { "<key>": { ... } } } }
        # Mirrors the Rust loader. Multiple types per file → multiple
        # top-level keys.
        if not isinstance(root, dict):
            errors.append(f"{rel}: top-level must be an object keyed by recipe type")
            continue
        for btype, by_category in root.items():
            if not isinstance(by_category, dict):
                errors.append(f"{rel}: type {btype!r}: value not an object")
                continue
            for category, recipes_obj in by_category.items():
                if not isinstance(recipes_obj, dict):
                    errors.append(f"{rel}: {btype}/{category}: value not an object of recipe keys")
                    continue
                for rid in recipes_obj:
                    if rid in all_ids:
                        prev = all_ids[rid]
                        errors.append(
                            f"Duplicate recipe id '{rid}' in {rel}"
                            f" (first seen under '{prev[0]}/{prev[1]}')"
                        )
                    else:
                        all_ids[rid] = (btype, category)
                        by_type_cat.setdefault((btype, category), []).append(rid)

    if errors:
        for e in errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        return False

    result: dict = {}
    n_active = n_added = n_removed = 0

    for (type_name, category_name) in sorted(by_type_cat):
        ids = by_type_cat[(type_name, category_name)]
        cat_existing = existing.get(type_name, {}).get(category_name, {})
        cat_result = dict(cat_existing)
        nid = next_id(cat_result)

        for rid in ids:
            if rid not in cat_result:
                cat_result[rid] = nid
                if skip_known:
                    print(f"    + {type_name}/{category_name}/{rid} = {nid}")
                n_added += 1
                nid += 1

        if skip_known:
            removed = [k for k in cat_existing if k not in set(ids)]
            for r in removed:
                print(f"  WARNING: '{type_name}/{category_name}/{r}' (id={cat_existing[r]}) no longer in source — ID reserved")
            n_removed += len(removed)
        n_active += len(ids)

        result.setdefault(type_name, {})[category_name] = dict(
            sorted(cat_result.items(), key=lambda kv: kv[1])
        )

    if skip_known:
        # Preserve tombstoned types/categories entirely.
        for type_name, cats in existing.items():
            for category_name, entries in cats.items():
                if type_name not in result or category_name not in result.get(type_name, {}):
                    print(f"  WARNING: '{type_name}/{category_name}' no longer in source — IDs reserved")
                    result.setdefault(type_name, {})[category_name] = entries

    with open(id_path, "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
        f.write("\n")

    print(f"  {n_active} active, {n_added} new, {n_removed} removed")
    return True


# ── Cards ──────────────────────────────────────────────────────────────────────

MAX_DEFINITION_ID = 255  # u8; 0 is sentinel


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

    # Collect cards grouped by (type, category), checking for duplicate keys globally.
    by_type_cat: dict[tuple[str, str], list[str]] = {}
    all_keys: dict[str, tuple[str, str]] = {}  # key -> (type, category) for duplicate check
    errors: list[str] = []

    for card_file in sorted(defs_dir.rglob("*.json")):
        rel = card_file.relative_to(defs_dir).as_posix()
        try:
            root = load_json(card_file)
        except json.JSONDecodeError as e:
            errors.append(f"{rel}: JSON parse error: {e}")
            continue
        # Nested shape:
        #   { "<card_type>": { "<category>": { "<key>": { ... } } } }
        # Mirrors the Rust loader. Multiple types per file → multiple
        # top-level keys.
        if not isinstance(root, dict):
            errors.append(f"{rel}: top-level must be an object keyed by card_type")
            continue
        for type_name, by_category in root.items():
            if not isinstance(by_category, dict):
                errors.append(f"{rel}: type {type_name!r}: value not an object")
                continue
            for category_name, cards_obj in by_category.items():
                if not isinstance(cards_obj, dict):
                    errors.append(f"{rel}: {type_name}/{category_name}: category value not an object")
                    continue
                for key in cards_obj:
                    if key in all_keys:
                        prev = all_keys[key]
                        errors.append(
                            f"Duplicate card key '{key}' in {rel}"
                            f" (first seen under '{prev[0]}/{prev[1]}')"
                        )
                    else:
                        all_keys[key] = (type_name, category_name)
                        by_type_cat.setdefault((type_name, category_name), []).append(key)

    if errors:
        for e in errors:
            print(f"  ERROR: {e}", file=sys.stderr)
        return False

    result: dict = {}
    n_active = n_added = n_removed = 0

    for (type_name, category_name) in sorted(by_type_cat):
        keys = by_type_cat[(type_name, category_name)]
        cat_existing = existing.get(type_name, {}).get(category_name, {})
        cat_result = dict(cat_existing)
        nid = next_id(cat_result)

        for key in keys:
            if key not in cat_result:
                if nid > MAX_DEFINITION_ID:
                    print(
                        f"  ERROR: '{type_name}/{category_name}' definition_id {nid} exceeds u8 max ({MAX_DEFINITION_ID})",
                        file=sys.stderr,
                    )
                    return False
                cat_result[key] = nid
                if skip_known:
                    print(f"    + {type_name}/{category_name}/{key} = {nid}")
                n_added += 1
                nid += 1

        if skip_known:
            removed = [k for k in cat_existing if k not in set(keys)]
            for r in removed:
                print(f"  WARNING: '{type_name}/{category_name}/{r}' (id={cat_existing[r]}) no longer in source — ID reserved")
            n_removed += len(removed)
        n_active += len(keys)

        result.setdefault(type_name, {})[category_name] = dict(
            sorted(cat_result.items(), key=lambda kv: kv[1])
        )

    if skip_known:
        # Preserve tombstoned types/categories entirely.
        for type_name, cats in existing.items():
            for category_name, entries in cats.items():
                if type_name not in result or category_name not in result.get(type_name, {}):
                    print(f"  WARNING: '{type_name}/{category_name}' no longer in source — IDs reserved")
                    result.setdefault(type_name, {})[category_name] = entries

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
                for pack_id in packs_obj:
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
    Shape: `<card_type>.<category>.<key>`."""
    return _walk_nested_paths(data_dir / "cards" / "data", depth=3)


def walk_recipe_data_paths(data_dir: Path) -> set[str]:
    """Dotted paths for every recipe declared under `recipes/data/`.
    Shape: `<recipe_type>.<category>.<key>`."""
    return _walk_nested_paths(data_dir / "recipes" / "data", depth=3)


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


def _collect_locale_paths(node: dict, so_far: list[str], out: set[str]) -> None:
    """Walk a locale file and emit a dotted path for every leaf
    (an object with a `label` field)."""
    if "label" in node and isinstance(node.get("label"), str):
        out.add(".".join(so_far))
        return
    for k, v in node.items():
        if not isinstance(v, dict):
            continue
        so_far.append(k)
        _collect_locale_paths(v, so_far, out)
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
        _collect_locale_paths(root, [], existing_paths)

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

    print("Locales:")
    if not sync_locales(data_dir, verbose=args.verbose, purge_orphans=args.purge_orphans):
        ok = False

    if not ok:
        sys.exit(1)

    print("Done.")


if __name__ == "__main__":
    main()
