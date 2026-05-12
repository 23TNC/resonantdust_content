//! Card / action / magnetic_action flag-bit registries.
//!
//! Embeds `cards/flags.json` at compile time and exposes two lookup
//! flavours per top-level domain:
//!
//! - **single-bit flags** (JSON entries with `"bit": <n>`) — surfaced via
//!   [`card_flag_bit`] which returns the bit position (0..=31). Callers
//!   typically build a mask via `1u32 << bit`.
//! - **multi-bit fields** (JSON entries with `"bits": [<low>, …, <high>]`,
//!   contiguous and ascending) — surfaced via [`card_flag_field`] which
//!   returns a [`FlagField`] knowing the field's `shift` (= lowest bit
//!   position) and `width` (= bit count). Use [`FlagField::pack`] to
//!   shift a value into the field's bit window, [`FlagField::mask`] to
//!   clear the field before re-writing.
//!
//! Currently only the `cards` section is surfaced; `actions` /
//! `magnetic_actions` follow the same shape and can be added by mirroring
//! the `cards_*` accessors.
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

/// Multi-bit field within a host integer (e.g. `progress_style` at bits
/// `[8, 9, 10]` in `Card.flags`). The JSON schema requires bit positions
/// to be contiguous and ascending (low-to-high), so a field is fully
/// described by its lowest bit position (`shift`) and the number of bits
/// (`width`).
///
/// Use [`FlagField::pack`] to convert a small integer value into the
/// bit-shifted form ready to OR into the host integer; use
/// [`FlagField::mask`] to clear the field's window before writing a new
/// value (so a stale prior value doesn't bleed into the new one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagField {
  /// Position of the lowest bit (= `bits[0]`).
  pub shift: u8,
  /// Number of bits the field occupies (= `bits.len()`).
  pub width: u8,
}

impl FlagField {
  /// Bitmask covering this field's window in the host integer. ANDing
  /// `!field.mask()` against an existing `flags` value clears the field.
  pub fn mask(self) -> u32 {
    self.value_mask() << self.shift
  }

  /// Pack a small integer value into this field's bit positions. Values
  /// wider than the field are silently truncated (callers that want a
  /// hard error on overflow should validate against
  /// `1 << field.width` themselves).
  pub fn pack(self, value: u32) -> u32 {
    (value & self.value_mask()) << self.shift
  }

  /// Mask covering the value before shifting (= `(1 << width) - 1`).
  fn value_mask(self) -> u32 {
    // Use u64 arithmetic so width=32 doesn't overflow the shift.
    (((1u64 << self.width) - 1) & 0xFFFF_FFFF) as u32
  }
}

struct FlagsRegistry {
  /// Single-bit `cards.flags` entries (JSON `"bit": <n>`). Value is the
  /// bit position (0..=31).
  cards: BTreeMap<String, u8>,
  /// Multi-bit `cards.flags` entries (JSON `"bits": [<low>, …, <high>]`).
  cards_fields: BTreeMap<String, FlagField>,
}

static FLAGS: OnceLock<Result<FlagsRegistry, String>> = OnceLock::new();

fn flags_registry() -> Result<&'static FlagsRegistry, String> {
  FLAGS.get_or_init(build_flags).as_ref().map_err(|e| e.clone())
}

/// Look up a single-bit card-flag's bit position (0..=31) by name.
/// Returns `Ok(None)` if no single-bit flag with that name is declared
/// in `cards/flags.json`'s `cards` section, `Err` if the flags registry
/// failed to build. Multi-bit fields (`bits: [...]` JSON entries) are
/// not surfaced here — use [`card_flag_field`] for those.
pub fn card_flag_bit(name: &str) -> Result<Option<u8>, String> {
  Ok(flags_registry()?.cards.get(name).copied())
}

/// Look up a multi-bit card-flag field by name. Returns `Ok(None)` if
/// no multi-bit field with that name is declared in
/// `cards/flags.json`'s `cards` section, `Err` if the flags registry
/// failed to build. Single-bit flags (`bit: n` JSON entries) are not
/// surfaced here — use [`card_flag_bit`] for those.
pub fn card_flag_field(name: &str) -> Result<Option<FlagField>, String> {
  Ok(flags_registry()?.cards_fields.get(name).copied())
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
  let mut cards_fields: BTreeMap<String, FlagField> = BTreeMap::new();
  for (name, info) in cards_obj {
    if name.starts_with('_') {
      continue;
    }
    let info_obj = info.as_object().ok_or_else(|| {
      format!("cards/flags.json: cards.{:?} not an object", name)
    })?;

    let has_bit = info_obj.get("bit").is_some();
    let has_bits = info_obj.get("bits").is_some();
    if has_bit && has_bits {
      return Err(format!(
        "cards/flags.json: cards.{:?} declares both 'bit' and 'bits' — pick one",
        name,
      ));
    }

    if has_bit {
      let bit_value = &info_obj["bit"];
      let bit = bit_value.as_u64().ok_or_else(|| {
        format!("cards/flags.json: cards.{:?} 'bit' not an integer", name)
      })?;
      // Card.flags is u32 → bit positions 0..=31.
      if bit > 31 {
        return Err(format!(
          "cards/flags.json: cards.{:?} bit {} exceeds u32 max (31)",
          name, bit,
        ));
      }
      cards.insert(name.clone(), bit as u8);
    } else if has_bits {
      let bits_arr = info_obj["bits"].as_array().ok_or_else(|| {
        format!("cards/flags.json: cards.{:?} 'bits' not an array", name)
      })?;
      if bits_arr.is_empty() {
        return Err(format!(
          "cards/flags.json: cards.{:?} 'bits' is an empty array",
          name,
        ));
      }
      // Collect, validate each is u64, in u32 range, contiguous ascending.
      let mut positions: Vec<u8> = Vec::with_capacity(bits_arr.len());
      for (i, v) in bits_arr.iter().enumerate() {
        let p = v.as_u64().ok_or_else(|| {
          format!(
            "cards/flags.json: cards.{:?} bits[{}] not a non-negative integer",
            name, i
          )
        })?;
        if p > 31 {
          return Err(format!(
            "cards/flags.json: cards.{:?} bits[{}] = {} exceeds u32 max (31)",
            name, i, p,
          ));
        }
        if i > 0 && p as u8 != positions[i - 1] + 1 {
          return Err(format!(
            "cards/flags.json: cards.{:?} bits must be contiguous low-to-high \
             (got bits[{}] = {} after bits[{}] = {})",
            name, i, p, i - 1, positions[i - 1],
          ));
        }
        positions.push(p as u8);
      }
      let field = FlagField {
        shift: positions[0],
        width: positions.len() as u8,
      };
      cards_fields.insert(name.clone(), field);
    }
    // Entries with neither `bit` nor `bits` are ignored — keeps the
    // schema friendly to documentation-only stub entries if any ever
    // land. (Today every real entry has one or the other.)
  }

  Ok(FlagsRegistry { cards, cards_fields })
}
