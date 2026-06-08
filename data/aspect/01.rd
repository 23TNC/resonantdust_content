; content/data/aspect/01.rd
; The aspect catalog — replaces cards/aspects.json. Each aspect is a flat record.
; A `satisfies` LUT (this aspect IS-A those aspects) inverts the old parent tree,
; so an aspect can satisfy several at once (pine could be wood AND conifer). A
; recipe predicate `aspect.Q` folds over a card: sum every stocked aspect A where
; A==Q or Q in satisfies(A). The gate computes the transitive closure + sum and
; writes the rolled-up `aspect.Q` into the operating-set frame; the VM just reads
; the key. Exact match is implicit — no aspect lists itself.
;
; `section`: aspects (what a card HAS — recipe inputs) / features (how it ACTS —
; behavioural tags) / traits (what it IS — sim-only, not displayed). icon/color
; are display; a record that omits them inherits from the first aspect it
; satisfies (so `pine` shows wood's 🪵). Labels/descriptions live in
; locales/aspects/<lang>.json, keyed by aspect name.

<aspect>

  ; --- aspects: resources, materials, soul stats (primary recipe inputs) ---
  ::wood>
    @define>
      aspects &section set
      🪵 &icon set
      #6B4423 &color set
  ::pine>
    @define>
      aspects &section set
      1 &satisfies array
      $aspect::wood &satisfies.0 set
      $asset::pine &art set        ; render sprite — ring_objects pulls this

  ::stone>
    @define>
      aspects &section set
      🪨 &icon set
      #808080 &color set
      $asset::stone &art set

  ::metal>
    @define>
      aspects &section set
      ⬛ &icon set
      #A0A0A0 &color set

  ::food>
    @define>
      aspects &section set
      🌾 &icon set
      #CCAA22 &color set
  ::berry>
    @define>
      aspects &section set
      1 &satisfies array
      $aspect::food &satisfies.0 set
      $asset::berry &art set

  ::flora>
    @define>
      aspects &section set
      🌿 &icon set
      #4A8B3E &color set
      $asset::flora &art set
  ::brush>
    @define>
      aspects &section set
      1 &satisfies array
      $aspect::flora &satisfies.0 set

  ::fire>
    @define>
      aspects &section set
      🔥 &icon set
      #CC3311 &color set
  ::fuel>
    @define>
      aspects &section set
      1 &satisfies array
      $aspect::fire &satisfies.0 set

  ::water>
    @define>
      aspects &section set
      💧 &icon set
      #1E6B9E &color set

  ::corpse>
    @define>
      aspects &section set
      💀 &icon set
      #3a3a3a &color set

  ::corpus>
    @define>
      aspects &section set
      🜔 &icon set
      #E67E7E &color set
  ::corpus_lit>
    @define>
      aspects &section set
      1 &satisfies array
      $aspect::corpus &satisfies.0 set
  ::corpus_dim>
    @define>
      aspects &section set
      1 &satisfies array
      $aspect::corpus &satisfies.0 set
  ::corpus_upgrade>
    @define>
      aspects &section set
      1 &satisfies array
      $aspect::corpus &satisfies.0 set

  ::anima>
    @define>
      aspects &section set
      🜍 &icon set
      #4040a0 &color set

  ::sollertia>
    @define>
      aspects &section set
      🜻 &icon set
      #97e3a7 &color set

  ::aether>
    @define>
      aspects &section set
      🜕 &icon set
      #ffb333 &color set

  ::soul>
    @define>
      aspects &section set
      ♙ &icon set
      #4455AA &color set

  ; --- features: behavioural / functional tags (recipe-matchable too) ---
  ::crafting>
    @define>
      features &section set
      🔨 &icon set
      #B07A3A &color set
  ::builder>
    @define>
      features &section set
      1 &satisfies array
      $aspect::crafting &satisfies.0 set

  ::fleeting>
    @define>
      features &section set
      ⌛ &icon set
      #556677 &color set

  ::level>
    @define>
      features &section set
      🎚 &icon set
      #C0A060 &color set

  ::faction>
    @define>
      features &section set
      🚩 &icon set
      #888888 &color set
  ::chorus>
    @define>
      features &section set
      1 &satisfies array
      $aspect::faction &satisfies.0 set
  ::chord>
    @define>
      features &section set
      1 &satisfies array
      $aspect::faction &satisfies.0 set
  ::resonance>
    @define>
      features &section set
      1 &satisfies array
      $aspect::faction &satisfies.0 set

  ::speed>
    @define>
      features &section set
      💨 &icon set
      #9DCEE0 &color set

  ::inventory>
    @define>
      features &section set
      🎒 &icon set
      #B07A3A &color set

  ; --- traits: descriptive sim properties, not displayed (no icon/color) ---
  ; `type` is the card's kind (tile/faculty/soul/…) — a symbol-valued classifier,
  ; not a magnitude: read by `eq` like def_id, never summed, no satisfies edges.
  ; Its presence here just lets `&aspect.type set` pass the aspect-member lint.
  ::type>
    @define>
      traits &section set
  ; `def_id` optionally PINS a card's per-type def id (the low 12 bits of its
  ; packed_definition) instead of letting the loader auto-increment — e.g. the
  ; player_soul sets it to 4095 (0xFFF) so its packed def is 0xFFFF. Loader-read
  ; only (like `type`); never summed. Declared here to pass the aspect-member lint.
  ::def_id>
    @define>
      traits &section set
  ; `progress` is a per-card stock counter (build/charge progress, 0..N). Backed
  ; by a card's `stock` u32 (declared `<bits> &aspect.progress stock`), read +
  ; written by recipes (`*root.aspect.progress`, `&root.aspect.progress inc`). A
  ; magnitude, but sim-only — not displayed.
  ::progress>
    @define>
      traits &section set
  ; Stacking bit-fields (bit i = stack i: 0 hex/under, 1 top, 2 bottom).
  ; stack_hosts = stacks this card sources as root; stack_joins = stacks it can
  ; occupy. Absent => client default (regular card: hosts 0b111, joins 0b110).
  ::stack_hosts>
    @define>
      traits &section set
  ::stack_joins>
    @define>
      traits &section set
  ::cost>
    @define>
      traits &section set
  ::height>
    @define>
      traits &section set
  ::elevation_min>
    @define>
      traits &section set
  ::elevation_max>
    @define>
      traits &section set
  ::temperature_min>
    @define>
      traits &section set
  ::temperature_max>
    @define>
      traits &section set
  ::humidity_min>
    @define>
      traits &section set
  ::humidity_max>
    @define>
      traits &section set
  ::aether_min>
    @define>
      traits &section set
  ::aether_max>
    @define>
      traits &section set
  ::rarity>
    @define>
      traits &section set
  ::coverage>
    @define>
      traits &section set
  ::cluster_group>
    @define>
      traits &section set
  ::cluster_strength>
    @define>
      traits &section set
  ; Anchor reach: per-tier tile radii a card projects to drive the client's zone
  ; subscription/memory tiers (active > hot > warm > cold). A card with these
  ; (typically a soul) makes the zones within each radius active/hot/warm/cold;
  ; cards subscribe within anchor_hot, zones within anchor_cold. Client-only —
  ; the shard/gate never read these. Distances are in tiles.
  ::anchor_active>
    @define>
      traits &section set
  ::anchor_hot>
    @define>
      traits &section set
  ::anchor_warm>
    @define>
      traits &section set
  ::anchor_cold>
    @define>
      traits &section set
