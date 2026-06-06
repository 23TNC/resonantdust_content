; content/data/cards/status.card
; Port of cards/data/status/mental.json — revery (rect) + event (hex) cards.
; `magnetic.{recipe,duration}` has no op yet; expressed as flagged slot sets.

<card>
  ::dread>
    :data>
      @define>
        revery &aspect.type set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a4a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set
      @init>
        $functions::rect_card call drop
      @update>
        $functions::rect_card call drop
      @destroy>
        $functions::rect_card call drop

  ::test>
    :data>
      @define>
        event &aspect.type set
        1 &aspect.stack_joins set
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set
      @init>
        $functions::hex_card call drop
      @update>
        $functions::hex_card call drop
      @destroy>
        $functions::hex_card call drop

  ::despair>
    :data>
      @define>
        event &aspect.type set
        1 &aspect.stack_joins set
        $recipe::despair_success &magnetic.recipe set   ; FLAG magnetic has no op — guessed slot set
        60000 &magnetic.duration set                     ; FLAG magnetic duration_ms
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set
      @init>
        $functions::hex_card call drop
      @update>
        $functions::hex_card call drop
      @destroy>
        $functions::hex_card call drop

  ::strike>
    :data>
      @define>
        event &aspect.type set
        1 &aspect.stack_joins set
        $recipe::strike_success &magnetic.recipe set    ; FLAG magnetic has no op — guessed slot set
        60000 &magnetic.duration set                     ; FLAG magnetic duration_ms
    :visuals>
      @define>
        $shape.generic &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set
      @init>
        $functions::hex_card call drop
      @update>
        $functions::hex_card call drop
      @destroy>
        $functions::hex_card call drop
