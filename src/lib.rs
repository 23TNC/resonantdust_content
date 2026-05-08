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

pub mod packed;
pub mod definition_core;
pub mod recipe_core;

#[cfg(feature = "js")]
mod wasm_api;
