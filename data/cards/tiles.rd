; content/data/cards/tiles.card
; Type `tile` — per-hex terrain. Port of cards/data/tiles/*.json.
;
; Terrains with rarity tiers (forest/plains/desert/mountain) are FOLDED into a
; single card each: @init buckets on *biome.rarity (0-10 / 10-20 / 20-30 / def)
; pick which stock aspects exist, `within if .. range` climate-gates them, and
; :norm normalizes each from its biome axis. Folding keeps one representative
; style per terrain (per-tier tints are lost — accepted).
;
; ring_objects renders stock counts as objects, pulling each aspect's sprite from
; the <aspect> registry (`art`) — so a card just stocks aspects; no per-card asset
; list. It iterates every aspect; ones with no `art` (type/cost, or resources
; without a pack) yield 0 textures and are skipped, so aspect order is irrelevant.
;
; Buildings are not folded — each is a distinct placeable with its own object.
; &texture holds a ground-texture asset (concrete), parallel to &shape/&color.

<card>
  ::empty>
    :data>
      @define>
        tile &aspect.type set
        10 &aspect.cost set
    :visuals>
      @define>
        $shape.hex &shape set
        #0b1426 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set

  ::concrete>
    :data>
      @define>
        tile &aspect.type set
        10 &aspect.cost set
    :visuals>
      @define>
        $shape.hex &shape set
        #9b9a96 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
        $asset::concrete &texture set

  ::forest>
    :data>
      @define>
        2    &aspect.pine stock
        2    &aspect.flora stock
        2    &aspect.stone stock
        tile &aspect.type set
        30   &aspect.cost set
      @init>
        ^biome call &biome set

        *biome.rarity 0 10 within !if :r10 goto
        *biome.humidity 65 85 within if 0 1 &aspect.pine range
        *biome.humidity 40 85 within if 0 2 &aspect.flora range
        :norm goto

        :r10>
        *biome.rarity 10 20 within !if :r20 goto
        *biome.humidity 75 95 within if 0 3 &aspect.pine range
        0 1 &aspect.flora range
        :norm goto

        :r20>
        *biome.rarity 20 30 within !if :def goto
        *biome.elevation 45 75 within if 0 3 &aspect.stone range
        *biome.humidity 55 80 within if 0 2 &aspect.flora range
        :norm goto

        :def>
        *biome.humidity 55 75 within if 0 3 &aspect.pine range
        *biome.humidity 50 80 within if 0 1 &aspect.flora range

        :norm>
        &aspect.pine *biome.humidity normalize
        &aspect.flora *biome.humidity normalize
        &aspect.stone *biome.elevation normalize
    :visuals>
      @define>
        $shape.hex &shape set
        #395C39 &color.bg set
        #2A2A2A &color.title set
        #0B1426 &color.text set
      @update>
        $functions:ring_objects call drop

  ::plains>
    :data>
      @define>
        2 &aspect.flora stock
        2 &aspect.berry stock
        tile &aspect.type set
        5 &aspect.cost set
      @init>
        ^biome call &biome set

        *biome.rarity 0 10 within !if :r10 goto
        0 1 &aspect.flora range
        0 1 &aspect.berry range
        :norm goto

        :r10>
        *biome.rarity 10 20 within !if :r20 goto
        0 2 &aspect.flora range
        0 1 &aspect.berry range
        :norm goto

        :r20>
        *biome.rarity 20 30 within !if :def goto
        *biome.humidity 30 50 within if 0 3 &aspect.berry range
        0 1 &aspect.flora range
        :norm goto

        :def>
        *biome.humidity 40 55 within if 0 3 &aspect.flora range
        *biome.humidity 40 55 within if 0 2 &aspect.berry range

        :norm>
        &aspect.flora *biome.humidity normalize
        &aspect.berry *biome.humidity normalize
    :visuals>
      @define>
        $shape.hex &shape set
        #C9D75F &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
      @update>
        $functions:ring_objects call drop

  ::desert>
    :data>
      @define>
        2    &aspect.stone stock
        1    &aspect.flora stock
        2    &aspect.water stock
        2    &aspect.food  stock
        2    &aspect.fuel  stock
        tile &aspect.type  set
        12   &aspect.cost  set
      @init>
        ^biome call &biome set

        *biome.rarity 0 10 within !if :r10 goto
        *biome.elevation 30 65 within if 0 1 &aspect.stone range
        *biome.humidity 0 30 within if 0 1 &aspect.flora range
        :norm goto

        :r10>
        *biome.rarity 10 20 within !if :r20 goto
        *biome.elevation 30 65 within if 0 2 &aspect.stone range
        *biome.humidity 0 25 within if 0 1 &aspect.flora range
        :norm goto

        :r20>
        *biome.rarity 20 30 within !if :def goto
        *biome.humidity 15 30 within if 0 3 &aspect.water range
        *biome.humidity 15 30 within if 0 2 &aspect.food range
        :norm goto

        :def>
        *biome.elevation 30 55 within if 0 3 &aspect.stone range
        *biome.temperature 80 100 within if 0 2 &aspect.fuel range

        :norm>
        &aspect.stone *biome.elevation normalize
        &aspect.flora *biome.humidity normalize
        &aspect.water *biome.humidity normalize
        &aspect.food *biome.humidity normalize
        &aspect.fuel *biome.temperature normalize
    :visuals>
      @define>
        $shape.hex &shape set
        #D4A464 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
      @update>
        $functions:ring_objects call drop

  ::mountain>
    :data>
      @define>
        2 &aspect.stone stock
        1 &aspect.flora stock
        2 &aspect.metal stock
        tile &aspect.type set
        15 &aspect.cost set
      @init>
        ^biome call &biome set
        
        *biome.rarity 0 10 within !if :r10 goto
        *biome.elevation 70 100 within if 0 2 &aspect.stone range
        0 1 &aspect.flora range
        :norm goto

        :r10>
        *biome.rarity 10 20 within !if :r20 goto
        *biome.elevation 75 100 within if 0 2 &aspect.stone range
        *biome.elevation 75 100 within if 0 1 &aspect.metal range
        :norm goto

        :r20>
        *biome.rarity 20 30 within !if :def goto
        *biome.elevation 80 100 within if 0 3 &aspect.stone range
        *biome.elevation 80 100 within if 0 2 &aspect.metal range
        :norm goto

        :def>
        *biome.elevation 90 100 within if 0 3 &aspect.stone range
        *biome.humidity 20 80 within if 0 1 &aspect.flora range
        1 &trait.height set

        :norm>
        &aspect.stone *biome.elevation normalize
        &aspect.flora *biome.humidity normalize
        &aspect.metal *biome.elevation normalize
    :visuals>
      @define>
        $shape.hex &shape set
        #6B6859 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
      @update>
        $functions:ring_objects call drop

  ::building_nd_furnace>
    :data>
      @define>
        tile &aspect.type set
        2 &aspect.fire stock
        50 &aspect.cost set
    :visuals>
      @define>
        $shape.hex &shape set
        #799E50 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::nd_furnace &objects.0 set

  ::building_workbench>
    :data>
      @define>
        tile &aspect.type set
        2 &aspect.fire stock
        50 &aspect.cost set
    :visuals>
      @define>
        $shape.hex &shape set
        #799E50 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
        1 &objects array
        $asset::workbench &objects.0 set

  ::alter>
    :data>
      @define>
        tile &aspect.type set
        1 &aspect.level set
        50 &aspect.cost set
    :visuals>
      @define>
        $shape.hex &shape set
        #C0A060 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
        $asset::concrete &texture set
        1 &objects array
        $asset::alter &objects.0 set   ; frame from *aspect.level (was indexFromAspect)

  ::anima_fountain>
    :data>
      @define>
        tile &aspect.type set
        1 &aspect.anima set
        50 &aspect.cost set
    :visuals>
      @define>
        $shape.hex &shape set
        #FFD966 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
        $asset::concrete &texture set
        1 &objects array
        $asset::fountain:anima &objects.0 set

  ::aether_fountain>
    :data>
      @define>
        tile &aspect.type set
        1 &aspect.aether set
        50 &aspect.cost set
    :visuals>
      @define>
        $shape.hex &shape set
        #7D96E3 &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
        $asset::concrete &texture set
        1 &objects array
        $asset::fountain:aether &objects.0 set

  ::table>
    :data>
      @define>
        tile &aspect.type set
        30 &aspect.cost set
    :visuals>
      @define>
        $shape.hex &shape set
        #8B5E3C &color.bg set
        #0b1426 &color.title set
        #0b1426 &color.text set
        $asset::concrete &texture set
        1 &objects array
        $asset::table_slab &objects.0 set
