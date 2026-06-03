; content/data/cards/faculties.card
; Type `faculty` (rect). Port of cards/data/status/stats.json.
; The `corpus` faculty carries the corpus_lit aspect; _lit/_dim variants are
; mostly face-only (some carry their matching aspect, some carry only a color).

<card>
  ::corpus>
    :data>
      @define>
        faculty &aspect.type set
        1 &aspect.corpus_lit set
    :visuals>
      @define>
        $shape.rect &shape set
        #E67E7E &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::symbols:corpus &objects.0 set

  ::corpus_dim>
    :data>
      @define>
        faculty &aspect.type set
        1 &aspect.corpus_dim set
    :visuals>
      @define>
        $shape.rect &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set

  ::corpus_upgrade>
    :data>
      @define>
        faculty &aspect.type set
        1 &aspect.corpus_upgrade set
    :visuals>
      @define>
        $shape.rect &shape set
        #E67E7E &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set

  ::aether>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #7D96E3 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::symbols:aether &objects.0 set

  ::aether_dim>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set

  ::aether_lit>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #7D96E3 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set

  ::sollertia>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #97e3a7 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::symbols:sollertia &objects.0 set

  ::sollertia_dim>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set

  ::sollertia_lit>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #97e3a7 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set

  ::anima>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #FFD966 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::symbols:anima &objects.0 set

  ::anima_dim>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set

  ::anima_lit>
    :data>
      @define>
        faculty &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #FFD966 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
