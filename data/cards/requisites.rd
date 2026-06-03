; content/data/cards/requisites.card
; Type `requisite` (rect). Port of cards/data/requisites/resources.json +
; equipment.json. Static aspects; icon -> objects[0].

<card>
  ::log>
    :data>
      @define>
        requisite &aspect.type set
        2 &aspect.fuel set
        2 &aspect.wood set
    :visuals>
      @define>
        $shape.rect &shape set
        #8B5E3C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::requisite:log &objects.0 set

  ::stick>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.fuel set
        1 &aspect.wood set
    :visuals>
      @define>
        $shape.rect &shape set
        #8B5E3C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::requisite:stick &objects.0 set

  ::stone>
    :data>
      @define>
        requisite &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #8A857C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::requisite:stone &objects.0 set

  ::dust>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.aether set
    :visuals>
      @define>
        $shape.rect &shape set
        #ffb333 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::requisite:dust &objects.0 set

  ::food>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.food set
    :visuals>
      @define>
        $shape.rect &shape set
        #CCAA22 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::requisite:food &objects.0 set

  ::reliquary>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.anima set
    :visuals>
      @define>
        $shape.rect &shape set
        #4040a0 &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::requisite:reliquary &objects.0 set

  ::corpse>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.corpse set
        1 &aspect.inventory set
    :visuals>
      @define>
        $shape.rect &shape set
        #3a3a3a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::soul_offline &objects.0 set

  ::corpse_chorus>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.corpse set
        1 &aspect.inventory set
        1 &aspect.chorus set
    :visuals>
      @define>
        $shape.rect &shape set
        #3a3a3a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::soul_offline &objects.0 set

  ::corpse_chord>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.corpse set
        1 &aspect.inventory set
        1 &aspect.chord set
    :visuals>
      @define>
        $shape.rect &shape set
        #3a3a3a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::soul_offline &objects.0 set

  ::corpse_resonance>
    :data>
      @define>
        requisite &aspect.type set
        1 &aspect.corpse set
        1 &aspect.inventory set
        1 &aspect.resonance set
    :visuals>
      @define>
        $shape.rect &shape set
        #3a3a3a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::soul_offline &objects.0 set

  ::axe>
    :data>
      @define>
        requisite &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #8B5E3C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::requisite:axe &objects.0 set

  ::pickaxe>
    :data>
      @define>
        requisite &aspect.type set
    :visuals>
      @define>
        $shape.rect &shape set
        #8A857C &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::requisite:pickaxe &objects.0 set
