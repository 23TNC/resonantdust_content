#!/usr/bin/env python3
"""
gen-ids.py — Generate stable ID mappings for recipes and cards.

Produces:
  recipes/id.json  { "<type>": { "<category>": { "<recipe-id>": <stable-int>, ... } } }
  cards/id.json    { "<card_type>": { "<category>": { "<key>": <definition_id>, ... } } }

Reads recipe definition files from `recipes/data/*.json` and card definition
files from `cards/data/*.json`. Sibling metadata at `recipes/` and `cards/`
(types.json, aspects.json, flags.json, the id.json files themselves) is
ignored.

Both recipe and card IDs are assigned once and never reassigned. Adding or
reordering entries does not change existing IDs. Removed entries are kept as
reserved tombstones so their IDs are never recycled.

Recipe IDs are scoped per (type, category): stack/up, stack/down, on_create/self each
have their own counter starting at 1. Duplicate recipe ids across any type/category
are an error.

Card definition_ids are u8 (1–255) scoped per (card_type, category); 0 is reserved as
sentinel. The server combines a card's definition_id with its bucket's type
and category at build time to produce the packed_definition wire format.

Run before `spacetime build`.
"""

import json
import sys
from pathlib import Path


def load_json(path: Path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def next_id(existing: dict) -> int:
    return max(existing.values(), default=0) + 1


# ── Recipes ────────────────────────────────────────────────────────────────────

BUCKET_LISTS = {
    "stack":     ["up", "down"],
    "on_create": ["self"],
}


def gen_recipe_ids(data_dir: Path) -> bool:
    recipes_dir = data_dir / "recipes"
    defs_dir = recipes_dir / "data"
    id_path = recipes_dir / "id.json"

    raw = load_json(id_path) if id_path.exists() else {}
    # Detect old flat format { "recipe_id": int } and discard it.
    if raw and any(isinstance(v, int) for v in raw.values()):
        print("  NOTE: recipes/id.json is in old flat format — resetting to nested format")
        raw = {}
    existing: dict = raw

    by_type_cat: dict[tuple[str, str], list[str]] = {}
    all_ids: dict[str, tuple[str, str]] = {}  # id -> (type, category) for duplicate check
    errors: list[str] = []

    for recipe_file in sorted(defs_dir.glob("*.json")):
        try:
            buckets = load_json(recipe_file)
        except json.JSONDecodeError as e:
            errors.append(f"{recipe_file.name}: JSON parse error: {e}")
            continue
        if not isinstance(buckets, list):
            errors.append(f"{recipe_file.name}: expected a top-level array")
            continue
        for bucket in buckets:
            btype = bucket.get("type")
            if btype not in BUCKET_LISTS:
                errors.append(f"{recipe_file.name}: unknown bucket type {btype!r}")
                continue
            for list_key in BUCKET_LISTS[btype]:
                entries = bucket.get(list_key, [])
                if not isinstance(entries, list):
                    errors.append(f"{recipe_file.name}: '{btype}.{list_key}' is not an array")
                    continue
                for recipe in entries:
                    rid = recipe.get("id")
                    if not rid:
                        errors.append(f"{recipe_file.name}: recipe missing 'id' field: {recipe!r}")
                        continue
                    if rid in all_ids:
                        prev = all_ids[rid]
                        errors.append(
                            f"Duplicate recipe id '{rid}' in {recipe_file.name}"
                            f" (first seen under '{prev[0]}/{prev[1]}')"
                        )
                    else:
                        all_ids[rid] = (btype, list_key)
                        by_type_cat.setdefault((btype, list_key), []).append(rid)

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
                print(f"    + {type_name}/{category_name}/{rid} = {nid}")
                n_added += 1
                nid += 1

        removed = [k for k in cat_existing if k not in set(ids)]
        for r in removed:
            print(f"  WARNING: '{type_name}/{category_name}/{r}' (id={cat_existing[r]}) no longer in source — ID reserved")
        n_removed += len(removed)
        n_active += len(ids)

        result.setdefault(type_name, {})[category_name] = dict(
            sorted(cat_result.items(), key=lambda kv: kv[1])
        )

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


def gen_card_ids(data_dir: Path) -> bool:
    cards_dir = data_dir / "cards"
    defs_dir = cards_dir / "data"
    id_path = cards_dir / "id.json"

    existing: dict = load_json(id_path) if id_path.exists() else {}

    # Collect cards grouped by (type, category), checking for duplicate keys globally.
    by_type_cat: dict[tuple[str, str], list[str]] = {}
    all_keys: dict[str, tuple[str, str]] = {}  # key -> (type, category) for duplicate check
    errors: list[str] = []

    for card_file in sorted(defs_dir.glob("*.json")):
        try:
            buckets = load_json(card_file)
        except json.JSONDecodeError as e:
            errors.append(f"{card_file.name}: JSON parse error: {e}")
            continue
        if not isinstance(buckets, list):
            errors.append(f"{card_file.name}: expected a top-level array")
            continue
        for bucket in buckets:
            type_name = bucket.get("card_type")
            category_name = bucket.get("category", "default")
            if not type_name:
                continue
            cards_obj = bucket.get("cards", {})
            if not isinstance(cards_obj, dict):
                errors.append(f"{card_file.name}: 'cards' is not an object")
                continue
            for key in cards_obj:
                if key in all_keys:
                    prev = all_keys[key]
                    errors.append(
                        f"Duplicate card key '{key}' in {card_file.name}"
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
                print(f"    + {type_name}/{category_name}/{key} = {nid}")
                n_added += 1
                nid += 1

        removed = [k for k in cat_existing if k not in set(keys)]
        for r in removed:
            print(f"  WARNING: '{type_name}/{category_name}/{r}' (id={cat_existing[r]}) no longer in source — ID reserved")
        n_removed += len(removed)
        n_active += len(keys)

        result.setdefault(type_name, {})[category_name] = dict(
            sorted(cat_result.items(), key=lambda kv: kv[1])
        )

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


# ── Entry point ────────────────────────────────────────────────────────────────

def main():
    script_dir = Path(__file__).parent.resolve()
    data_dir = (script_dir).resolve()

    if not data_dir.exists():
        print(f"ERROR: data directory not found: {data_dir}", file=sys.stderr)
        sys.exit(1)

    ok = True

    print("Recipes:")
    if not gen_recipe_ids(data_dir):
        ok = False

    print("Cards:")
    if not gen_card_ids(data_dir):
        ok = False

    if not ok:
        sys.exit(1)

    print("Done.")


if __name__ == "__main__":
    main()
