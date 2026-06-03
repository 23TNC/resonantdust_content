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
        $shape.rect &shape set
        #3a3a4a &color.bg set
        #ecd6aa &color.title set
        #0b1426 &color.text set

  ::test>
    :data>
      @define>
        event &aspect.type set
    :visuals>
      @define>
        $shape.hex &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set

  ::despair>
    :data>
      @define>
        event &aspect.type set
        $recipe::despair_success &magnetic.recipe set   ; FLAG magnetic has no op — guessed slot set
        60000 &magnetic.duration set                     ; FLAG magnetic duration_ms
    :visuals>
      @define>
        $shape.hex &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set

  ::strike>
    :data>
      @define>
        event &aspect.type set
        $recipe::strike_success &magnetic.recipe set    ; FLAG magnetic has no op — guessed slot set
        60000 &magnetic.duration set                     ; FLAG magnetic duration_ms
    :visuals>
      @define>
        $shape.hex &shape set
        #3a3a4a &color.bg set
        #2a2a3a &color.title set
        #0b1426 &color.text set
