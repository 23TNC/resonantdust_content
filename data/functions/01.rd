; content/data/functions/01.rd
; Shared functions a card calls to populate its &objects array. The engine then
; dumb-walks &objects and renders each entry, reading the ASSET's own size /
; scale / anchor for the transform — so object/placement behavior lives in the
; DSL, not the engine. A card calls one of these, or none (custom behavior).

; place_art — the default: one object, the card's &art asset, centered. A lone
; object centers, and the asset's anchor sets the pivot, so there's nothing to
; position here. A card sets `$asset::<x> &art set` then calls this; a card that
; wants something else (a ring, an effect) skips it and fills &objects directly.
<functions:place_art>
  1 &objects array
  *art &objects.0 set

; ring_objects — scatter each stock aspect's count as objects, pulling the sprite
; from the ASPECT registry (no per-card asset list). For each aspect entry: get
; its name (`key` at the index), `recall` its <aspect> record, and place
; aspect[i] copies of a random texture variant of the aspect's `art` into the
; next free object slots, until the array is full. Aspects with no `art`
; (type/cost/resources lacking a pack) resolve to 0 textures and are skipped, so
; iterating ALL aspects is fine — order no longer matters. var.0 = next free slot
; (advances only on a real placement), var.2 = aspect index, var.1 =
; placed-this-aspect, var.3 = chosen texture, var.4 = texture count. art ->
; .object (manifest) -> :<faction> -> .texture -> [var.3] (object/texture are
; literal members; faction/indices interpolate).
<functions:ring_objects>
  7 &objects array
  0 &var.0 set
  0 &var.2 set
  ^seed call &seed set

  :aspect>
    *var.2 &aspect count ge if 0 ret
    *var.0 &objects count ge if 0 ret
    &aspect *var.2 key &name set
    *name aspect recall &rec set
    *rec.art.object:*faction.texture count &var.4 set
    *var.4 0 eq if :next goto
    0 &var.1 set

    :place>
      *var.1 *aspect.*var.2 ge if :next goto
      *var.0 &objects count ge if 0 ret
      *seed *var.0 add random *var.4 mod &var.3 set
      *rec.art.object:*faction.texture.*var.3 &objects.*var.0 set
      &var.0 inc
      &var.1 inc
      :place goto

    :next>
    &var.2 inc
    :aspect goto
