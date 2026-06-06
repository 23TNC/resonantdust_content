; content/data/cards/faculties.card
; Type `faculty` (generic rect). The corpus/aether/sollertia/anima faculties
; carry a symbols-pack icon ($asset::symbols, variant = the faculty name); the
; _dim/_lit/_upgrade variants are face-only (no &pack → rect_card's sprite hides
; itself, leaving body + title).
<card>
  ::corpus>
    :data>
      @define>
        faculty &aspect.type set
        1 &aspect.corpus_lit set
    :visuals>
      @define>
        $shape.generic &shape set
        #E67E7E &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::symbols &pack set
        corpus &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::corpus_dim>
    :data>
      @define>
        faculty &aspect.type set
        1 &aspect.corpus_dim set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::corpus_upgrade>
    :data>
      @define>
        faculty &aspect.type set
        1 &aspect.corpus_upgrade set
    :visuals>
      @define>
        $shape.generic &shape set
        #E67E7E &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::aether>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #7D96E3 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::symbols &pack set
        aether &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::aether_dim>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::aether_lit>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #7D96E3 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::sollertia>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #97e3a7 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::symbols &pack set
        sollertia &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::sollertia_dim>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::sollertia_lit>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #97e3a7 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::anima>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #FFD966 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        $asset::symbols &pack set
        anima &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::anima_dim>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::anima_lit>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #FFD966 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop
