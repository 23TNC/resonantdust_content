; content/data/cards/requisites.card
; Type `requisite` (generic rect). :data = static aspects; :visuals sets the
; shape (generic), colours, and the art source (&pack + &variant), then calls
; $functions::rect_card to build the body/title/art prims. The requisite pack is
; a variant LUT ($asset::requisite, variant = log/stick/…); the corpse cards use
; the single-sprite soul_offline pack (no variant).
<card>
  ::log>
    :data>
      @define>
        requisite &aspect.type set
        2 &aspect.fuel set
        2 &aspect.wood set
    :visuals>
      @define>
        $shape.generic &shape set
        #8B5E3C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::requisite &pack set
        log &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::stick>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.fuel set
        1 &aspect.wood set
    :visuals>
      @define>
        $shape.generic &shape set
        #8B5E3C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::requisite &pack set
        stick &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::stone>
    :data>
      @define>
        requisite &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #8A857C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::requisite &pack set
        stone &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::dust>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.aether set
    :visuals>
      @define>
        $shape.generic &shape set
        #ffb333 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::requisite &pack set
        dust &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::food>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.food set
    :visuals>
      @define>
        $shape.generic &shape set
        #CCAA22 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::requisite &pack set
        food &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::reliquary>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.anima set
    :visuals>
      @define>
        $shape.generic &shape set
        #4040a0 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::requisite &pack set
        reliquary &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::corpse>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.corpse set
        1 &aspect.inventory set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a3a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::soul_offline &pack set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::corpse_chorus>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.corpse set
        1 &aspect.inventory set
        1 &aspect.chorus set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a3a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::soul_offline &pack set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::corpse_chord>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.corpse set
        1 &aspect.inventory set
        1 &aspect.chord set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a3a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::soul_offline &pack set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::corpse_resonance>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.corpse set
        1 &aspect.inventory set
        1 &aspect.resonance set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a3a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::soul_offline &pack set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::axe>
    :data>
      @define>
        requisite &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #8B5E3C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::requisite &pack set
        axe &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::pickaxe>
    :data>
      @define>
        requisite &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #8A857C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::requisite &pack set
        pickaxe &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop
