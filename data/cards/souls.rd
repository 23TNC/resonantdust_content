; content/data/cards/souls.card
; Type `soul` (generic rect). Portrait = the soul pack (soul_white / soul), a
; single-sprite pack with no variant — so rect_card leaves the index unset and
; the client picks the portrait by seed (the card id), giving each soul its own
; face. (The resource-meter overlay is still engine chrome, not yet a prim.)
<card>
  ::human>
    :data>
      @define>
        soul &aspect.type set
        2 &aspect.soul set
        1 &aspect.builder set
        12 &aspect.speed set
        1 &aspect.inventory set
    :visuals>
      @define>
        $shape.generic &shape set
        #a8e0e6 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::soul_white &pack set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::human_builder>
    :data>
      @define>
        soul &aspect.type set
        2 &aspect.soul set
        1 &aspect.builder set
        10 &aspect.speed set
        1 &aspect.inventory set
    :visuals>
      @define>
        $shape.generic &shape set
        #a8e0e6 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::soul_white &pack set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::player_soul>
    :data>
      @define>
        soul &aspect.type set
        1 &aspect.soul set
        1 &aspect.inventory set
    :visuals>
      @define>
        $shape.generic &shape set
        #4455AA &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::soul &pack set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop
