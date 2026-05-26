//! Card flag-bit registry.
//!
//! Embeds `cards/flags.json` at compile time. Cards' flag space is
//! split across **two** host integers — `cards.flags_state` (state
//! flags, bit-diff propagated forward by `cards::write_at`) and
//! `cards.flags_bk` (bookkeeping flags + refcount fields, never
//! bit-diff propagated). Each field has its own bit-allocation space
//! starting at bit 0; the split removes the implicit
//! `BOOKKEEPING_FLAGS_MASK` separator in favor of a structural one.
//!
//! Two flavours per field:
//!
//! - **single-bit flags** (JSON `"bit": <n>`) — surfaced via
//!   [`flag_bit`] which returns the bit position (0..=31).
//! - **multi-bit fields** (JSON `"bits": [<low>, …, <high>]`,
//!   contiguous and ascending) — surfaced via [`flag_field`] which
//!   returns a [`FlagField`] knowing the field's `shift` (= lowest
//!   bit position) and `width` (= bit count).
//!
//! # Field names
//!
//! - `"cards_state"` — propagated state. Holds the flags that
//!   describe "what's true about the card now."
//! - `"cards_bk"` — bookkeeping + refcounts. Server-managed.
//!
//! # Failure mode
//!
//! `OnceLock<Result<…, String>>` — a malformed flags file fails the
//! build once and every subsequent lookup returns the cached error.
//!
//! # Legacy single-namespace API
//!
//! [`card_flag_bit`] and [`card_flag_field`] preserve the old
//! "search by name across the whole cards namespace" shape — they
//! search `cards_state` first then `cards_bk`. Provided for the
//! transition period while call sites are being migrated to the
//! field-aware [`flag_bit`] / [`flag_field`]. Will retire once the
//! schema split lands on the server side.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

const FLAGS_JSON: &str = include_str!("../cards/flags.json");

/// Multi-bit field within a host integer. JSON schema requires bit
/// positions to be contiguous and ascending, so the field is fully
/// described by its lowest bit position (`shift`) and bit count
/// (`width`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagField {
  /// Position of the lowest bit (= `bits[0]`).
  pub shift: u8,
  /// Number of bits the field occupies (= `bits.len()`).
  pub width: u8,
}

impl FlagField {
  /// Bitmask covering this field's window in the host integer. ANDing
  /// `!field.mask()` against an existing flags value clears the field.
  pub fn mask(self) -> u32 {
    self.value_mask() << self.shift
  }

  /// Pack a small integer value into this field's bit positions.
  /// Values wider than the field are silently truncated.
  pub fn pack(self, value: u32) -> u32 {
    (value & self.value_mask()) << self.shift
  }

  /// Mask covering the value before shifting (= `(1 << width) - 1`).
  fn value_mask(self) -> u32 {
    // u64 arithmetic so width=32 doesn't overflow the shift.
    (((1u64 << self.width) - 1) & 0xFFFF_FFFF) as u32
  }
}

/// Per-field entry tables. Each field (`cards_state` / `cards_bk`)
/// gets its own pair of (single-bit, multi-bit) maps so lookups never
/// confuse a flag that happens to share a name across fields.
struct FieldRegistry {
  bits: BTreeMap<String, u8>,
  fields: BTreeMap<String, FlagField>,
}

struct FlagsRegistry {
  state: FieldRegistry,
  bk: FieldRegistry,
}

static FLAGS: OnceLock<Result<FlagsRegistry, String>> = OnceLock::new();

fn flags_registry() -> Result<&'static FlagsRegistry, String> {
  FLAGS.get_or_init(build_flags).as_ref().map_err(|e| e.clone())
}

fn registry_for(field: &str) -> Result<&'static FieldRegistry, String> {
  let r = flags_registry()?;
  match field {
    "cards_state" => Ok(&r.state),
    "cards_bk" => Ok(&r.bk),
    other => Err(format!(
      "cards/flags.json: unknown field {other:?} (expected 'cards_state' or 'cards_bk')"
    )),
  }
}

// ---------- Field-aware API (preferred) ----------

/// Look up a single-bit flag's bit position (0..=31) in the given
/// field. Returns `Ok(None)` if no single-bit flag with that name is
/// declared in the given field, `Err` if the registry failed to build
/// or `field` isn't recognized.
pub fn flag_bit(field: &str, name: &str) -> Result<Option<u8>, String> {
  Ok(registry_for(field)?.bits.get(name).copied())
}

/// Look up a multi-bit field by name in the given field. Returns
/// `Ok(None)` if no multi-bit field with that name is declared.
pub fn flag_field(field: &str, name: &str) -> Result<Option<FlagField>, String> {
  Ok(registry_for(field)?.fields.get(name).copied())
}

// ---------- Legacy single-namespace API ----------

/// **Legacy** — search both fields (state first, then bk) for a
/// single-bit flag by name. Preferred over hard-coding the field on
/// call sites during the transition period; new code should use
/// [`flag_bit`] with an explicit field name. Returns the bit position
/// (0..=31) without indicating which field it lives in — fine when
/// the caller is also reading the combined `flags = (flags_state |
/// flags_bk)` projection, but ambiguous against the split fields.
pub fn card_flag_bit(name: &str) -> Result<Option<u8>, String> {
  let r = flags_registry()?;
  Ok(
    r.state
      .bits
      .get(name)
      .copied()
      .or_else(|| r.bk.bits.get(name).copied()),
  )
}

/// **Legacy** — search both fields for a multi-bit flag field by
/// name. See [`card_flag_bit`] for the same caveat about field
/// ambiguity.
pub fn card_flag_field(name: &str) -> Result<Option<FlagField>, String> {
  let r = flags_registry()?;
  Ok(
    r.state
      .fields
      .get(name)
      .copied()
      .or_else(|| r.bk.fields.get(name).copied()),
  )
}

// ---------- Build ----------

fn build_flags() -> Result<FlagsRegistry, String> {
  let root: Value = serde_json::from_str(FLAGS_JSON)
    .map_err(|e| format!("cards/flags.json: parse failed: {e}"))?;
  let root = root
    .as_object()
    .ok_or_else(|| "cards/flags.json: top-level not an object".to_string())?;

  let state = build_field(root, "cards_state")?;
  let bk = build_field(root, "cards_bk")?;
  Ok(FlagsRegistry { state, bk })
}

fn build_field(root: &serde_json::Map<String, Value>, field: &str) -> Result<FieldRegistry, String> {
  let section = root
    .get(field)
    .and_then(Value::as_object)
    .ok_or_else(|| {
      format!("cards/flags.json: {field:?} missing or not an object")
    })?;

  let mut bits: BTreeMap<String, u8> = BTreeMap::new();
  let mut fields: BTreeMap<String, FlagField> = BTreeMap::new();
  for (name, info) in section {
    if name.starts_with('_') {
      continue;
    }
    let info_obj = info.as_object().ok_or_else(|| {
      format!("cards/flags.json: {field}.{name:?} not an object")
    })?;

    let has_bit = info_obj.get("bit").is_some();
    let has_bits = info_obj.get("bits").is_some();
    if has_bit && has_bits {
      return Err(format!(
        "cards/flags.json: {field}.{name:?} declares both 'bit' and 'bits' — pick one"
      ));
    }

    if has_bit {
      let bit = info_obj["bit"].as_u64().ok_or_else(|| {
        format!("cards/flags.json: {field}.{name:?} 'bit' not an integer")
      })?;
      if bit > 31 {
        return Err(format!(
          "cards/flags.json: {field}.{name:?} bit {bit} exceeds u32 max (31)"
        ));
      }
      bits.insert(name.clone(), bit as u8);
    } else if has_bits {
      let bits_arr = info_obj["bits"].as_array().ok_or_else(|| {
        format!("cards/flags.json: {field}.{name:?} 'bits' not an array")
      })?;
      if bits_arr.is_empty() {
        return Err(format!(
          "cards/flags.json: {field}.{name:?} 'bits' is an empty array"
        ));
      }
      let mut positions: Vec<u8> = Vec::with_capacity(bits_arr.len());
      for (i, v) in bits_arr.iter().enumerate() {
        let p = v.as_u64().ok_or_else(|| {
          format!(
            "cards/flags.json: {field}.{name:?} bits[{i}] not a non-negative integer"
          )
        })?;
        if p > 31 {
          return Err(format!(
            "cards/flags.json: {field}.{name:?} bits[{i}] = {p} exceeds u32 max (31)"
          ));
        }
        if i > 0 && p as u8 != positions[i - 1] + 1 {
          return Err(format!(
            "cards/flags.json: {field}.{name:?} bits must be contiguous low-to-high \
             (got bits[{i}] = {p} after bits[{prev_i}] = {prev_p})",
            prev_i = i - 1,
            prev_p = positions[i - 1],
          ));
        }
        positions.push(p as u8);
      }
      fields.insert(
        name.clone(),
        FlagField {
          shift: positions[0],
          width: positions.len() as u8,
        },
      );
    }
  }
  Ok(FieldRegistry { bits, fields })
}
