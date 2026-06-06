; content/data/cards/blueprints.card
; Type `blueprint` (generic rect). Art = the blueprint pack, nd_furnace variant.

<card>
  ::blueprint_nd_furnace>
    :data>
      @define>
        blueprint &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #0A3D73 &color.bg set
        #0B4F8A &color.title set
        #E6F1FF &color.text set
        $asset::blueprint &pack set
        nd_furnace &variant set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop
