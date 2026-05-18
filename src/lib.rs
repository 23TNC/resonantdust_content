//! Resonant Dust shared content crate.
//!
//! Embeds the JSON catalogs at compile time and exposes typed registries.
//! Two consumers:
//!
//! - **SpacetimeDB module** — depends on this crate as a path-dep `rlib` and
//!   calls into the public API directly from `resonantdust_content::*`.
//! - **pixijs client** — consumes the same crate built as a wasm bundle via
//!   `wasm-pack build --features js --target web`. The `js` feature gates
//!   `wasm-bindgen` annotations so the SpacetimeDB build doesn't pull in
//!   browser-only glue.
//!
//! Layout:
//!
//! - [`packed`]      bit-packing helpers for the wire format columns
//!                   (`packed_definition`, `packed_recipe`, `valid_at`,
//!                   `macro_zone`, `micro_zone`, tile rows).
//! - [`definition_core`] card and aspect registries, built lazily from the
//!                   JSON catalogs in `content/cards/`.
//! - [`recipe_core`]   recipe registry, built lazily from the JSON catalogs
//!                   in `content/recipes/`. Resolves `"@<type>"` entities and
//!                   aspect names through `definition_core`.
//! - [`starter_pack_core`] starter-pack registry, built lazily from
//!                   `content/starter_packs/`. Each pack is a soul-scoped
//!                   bundle of card-key → count pairs; card keys are
//!                   resolved through `definition_core`.
//! - [`texture_core`]  texture registry, built lazily from
//!                   `content/textures/`. Each texture is a render hint
//!                   (`object`, `size`, `scale`) keyed by
//!                   `(card_type, aspect_name)` — cards of that type
//!                   carrying the aspect render with the entry's spec.
//!                   Aspect names resolve through `definition_core`.
//!                   Client-side only — the server loads but never
//!                   reads textures.

pub mod packed;
pub mod definition_core;
pub mod biome_core;
pub mod recipe_core;
pub mod recipe_statement;
pub mod recipe_tape;
pub mod starter_pack_core;
pub mod texture_core;
pub mod flags_core;
pub mod locales_core;

#[cfg(feature = "js")]
mod wasm_api;

/// `(rel_path, contents)` slices for every `*.json` under `cards/data/`,
/// `recipes/data/`, `starter_packs/data/`, `textures/data/`, and
/// `locales/`, populated at compile time by `build.rs`. The registries
/// iterate these instead of hard-coding filenames, so adding / removing
/// / renaming a data file needs no source edit — cargo notices via the
/// build script's `rerun-if-changed` hooks and the slice regenerates.
mod embedded_data {
  include!(concat!(env!("OUT_DIR"), "/data_files.rs"));
}
