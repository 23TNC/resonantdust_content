; content/data/functions/01.rd
; <globals> — shared constants the DSL + client read instead of hardcoding sizes
; in engine code. `$globals::id` resolves to the `&value` (a px size, a count,
; …). A later global can be a function of an earlier one (title_height from
; card_height). Card dimensions live here now, so the generic card sizes itself
; from content, not from RECT_CARD_* constants.
<globals>
  ::card_width>
    @define>
      72 &value set
  ::card_height>
    @define>
      96 &value set
  ; title bar is 25% of the card height — a function of card_height, not a magic
  ; number. (Was RECT_CARD_TITLE_HEIGHT = 24 = 96 * 25 / 100.)
  ::title_height>
    @define>
      $globals::card_height 25 mul 100 div &value set

  ; world hex cell — pointy-top, derived from the display radius. width = √3·r,
  ; height = 2·r (matches the client HexGrid). The tile body fills these px.
  ::hex_radius>
    @define>
      96 &value set
  ::hex_width>
    @define>
      $globals::hex_radius 3 sqrt mul &value set
  ::hex_height>
    @define>
      $globals::hex_radius 2 mul &value set

; Shared functions a card calls from its :visuals hooks. Two output shapes:
;   - &objects array (legacy world path): the engine dumb-walks it and renders
;     each entry via the decorator, reading the ASSET's size/scale/anchor.
;   - &prims array (generic path, see CONVENTIONS "VISUAL PRIMITIVES"): a full
;     primitive render-spec the client PrimitiveLayer reconciles. `ring_prims`
;     is the &prims twin of `ring_objects`.
; Either way object/placement behavior lives in the DSL, not the engine. A card
; calls one of these, or none (custom behavior).
;
; `<functions>` catalogues each `::name>` function the same way `<card>`
; catalogues `::name>` cards — one `<>`/`::` grammar across every space, so
; functions version like cards ($functions::ring_objects → lineage head).
<functions>

  ; place_art — the default: one object, the card's &art asset, centered. A lone
  ; object centers, and the asset's anchor sets the pivot, so there's nothing to
  ; position here. A card sets `$asset::<x> &art set` then calls this; a card that
  ; wants something else (a ring, an effect) skips it and fills &objects directly.
  ::place_art>
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
  ::ring_objects>
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

  ; ring_prims — the generic-path version of ring_objects. Instead of filling an
  ; &objects array (positioned client-side by the decorator), it emits a full
  ; &prims list the PrimitiveLayer reconciles: prims.0 = the hex tile body (tint
  ; color.bg), then one sprite prim per stock object on an evenly-spaced ring
  ; (trig). The sprite's texture is the aspect's render OBJECT name (`art.object`,
  ; e.g. `manifest::pine`); the CLIENT picks the LOD variant + faction (so no
  ; ^seed / faction deref here). Coords are card-space 0..100; ring radii (28/25)
  ; approximate the hex bbox — tune visually. Object index var.0 (0..6) → prim
  ; index var.5 = var.0+1.
  ::ring_prims>
    ; ^hex constructs the tile body prim engine-side and returns a handle (&h);
    ; we configure it through the handle. Each ^sprite likewise pushes a prim and
    ; hands back &h — push order IS paint order, so no index bookkeeping.
    ^hex call &h set
    0.0 0.0     &h.pos vec2
    100.0 100.0 &h.size vec2
    *color.bg   &h.tint set

    0 &var.0 set
    0 &var.2 set

    :aspect>
      *var.2 &aspect count ge if 0 ret
      *var.0 7 ge if 0 ret
      &aspect *var.2 key &name set
      *name aspect recall &rec set
      *rec.art.object 0 eq if :next goto
      ; object size DEFAULT comes from the aspect's asset (`*rec.art.size`,
      ; card-space % of the cell) — pine is bigger than flora because its asset
      ; says so. A card can override by setting its own size. var.3 = this
      ; aspect's size.
      *rec.art.size &var.3 set
      0 &var.1 set

      :place>
        *var.1 *aspect.*var.2 ge if :next goto
        *var.0 7 ge if 0 ret
        ^sprite call &h set
        *rec.art.object &h.texture set
        50.0 50.0 &h.anchor vec2
        *var.3 *var.3 &h.size vec2
        *var.0 7.0 div 2.0 mul pi mul &var.6 set
        50.0 28.0 *var.6 cos mul add &var.7 set
        50.0 25.0 *var.6 sin mul add &var.8 set
        *var.7 *var.8 &h.pos vec2
        &var.0 inc
        &var.1 inc
        :place goto

      :next>
      &var.2 inc
      :aspect goto

  ; rect_card — the generic-path version of a standard rectangular card face
  ; (the &prims twin of LayoutRectCard's body/title/art sub-layers). Emits three
  ; prims into &prims: 0 = body fill (tint *color.bg), 1 = title-bar fill across
  ; the top 25% (24/96 of the card height, tint *color.title), 2 = the card art
  ; sprite (texture *objects.0 — the client resolves the asset:variant to its
  ; folder+index), centred in the body below the title and fit to 85% of the
  ; body box. Coords are card-space 0..100; fills anchor top-left (default),
  ; the sprite anchors centre. A card calls this from `:visuals @init`/`@update`
  ; after setting *color.* + *objects.0; title TEXT is not emitted yet (the VM
  ; has no label-key slot — client chrome for now).
  ::rect_card>
    ^rect call   &h set       ; body fill — push order is paint order
    0.0 0.0      &h.pos vec2
    100.0 100.0  &h.size vec2
    *color.bg    &h.tint set

    ^rect call   &h set       ; title bar
    0.0 0.0      &h.pos vec2
    100.0 25.0   &h.size vec2
    *color.title &h.tint set

    ^sprite call &h set     ; card art
    ; texture object + variant index are resolved by the CARD into &art (it
    ; knows its variant): &art.texture = *pack.object, &art.index = *pack.texture.<v>.
    *art.texture &h.texture set
    *art.index   &h.index set
    50.0 62.5    &h.pos vec2
    85.0 63.75   &h.size vec2
    50.0 50.0    &h.anchor vec2
