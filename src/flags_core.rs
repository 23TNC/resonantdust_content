//! Card / action / magnetic_action flag-bit registries.
//!
//! Embeds `cards/flags.json` at compile time and exposes a name → bit
//! position lookup per top-level domain. Currently supports the `cards`
//! section; `actions` / `magnetic_actions` follow the same shape and can
//! be added by mirroring the `cards_*` accessor.
//!
//! # Failure mode
//!
//! Same `OnceLock<Result<…, String>>` pattern as `definition_core`: a
//! malformed flags file fails the build once and every subsequent lookup
//! returns the cached error.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

const FLAGS_JSON: &str = include_str!("../cards/flags.json");

struct FlagsRegistry {
  cards: BTreeMap<String, u8>,
}

static FLAGS: OnceLock<Result<FlagsRegistry, String>> = OnceLock::new();

fn flags_registry() -> Result<&'static FlagsRegistry, String> {
  FLAGS.get_or_init(build_flags).as_ref().map_err(|e| e.clone())
}

/// Look up a card-flag's bit position (0..=7) by name. Returns `Ok(None)`
/// if no flag with that name is declared in `cards/flags.json`'s `cards`
/// section, `Err` if the flags registry failed to build.
pub fn card_flag_bit(name: &str) -> Result<Option<u8>, String> {
  Ok(flags_registry()?.cards.get(name).copied())
}

fn build_flags() -> Result<FlagsRegistry, String> {
  let root: Value = serde_json::from_str(FLAGS_JSON)
    .map_err(|e| format!("cards/flags.json: parse failed: {}", e))?;
  let root = root
    .as_object()
    .ok_or_else(|| "cards/flags.json: top-level not an object".to_string())?;

  let cards_obj = root
    .get("cards")
    .and_then(Value::as_object)
    .ok_or_else(|| {
      "cards/flags.json: 'cards' missing or not an object".to_string()
    })?;

  let mut cards: BTreeMap<String, u8> = BTreeMap::new();
  for (name, info) in cards_obj {
    if name.starts_with('_') {
      continue;
    }
    let info_obj = info.as_object().ok_or_else(|| {
      format!("cards/flags.json: cards.{:?} not an object", name)
    })?;
    // Only single-bit entries are surfaced today; multi-bit fields with
    // `bits: [...]` are skipped (no caller for them yet).
    if let Some(bit_value) = info_obj.get("bit") {
      let bit = bit_value.as_u64().ok_or_else(|| {
        format!("cards/flags.json: cards.{:?} 'bit' not an integer", name)
      })?;
      if bit > 7 {
        return Err(format!(
          "cards/flags.json: cards.{:?} bit {} exceeds u8 max (7)",
          name, bit,
        ));
      }
      cards.insert(name.clone(), bit as u8);
    }
  }

  Ok(FlagsRegistry { cards })
}
