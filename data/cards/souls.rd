; content/data/cards/souls.card
; Type `soul` (rect). Port of cards/data/souls/*.json.

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
        $shape.rect &shape set
        #a8e0e6 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::soul_white &objects.0 set

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
        $shape.rect &shape set
        #a8e0e6 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::soul_white &objects.0 set

  ::player_soul>
    :data>
      @define>
        soul &aspect.type set
        1 &aspect.soul set
        1 &aspect.inventory set
    :visuals>
      @define>
        $shape.rect &shape set
        #4455AA &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::soul &objects.0 set
