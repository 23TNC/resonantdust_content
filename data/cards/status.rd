; content/data/cards/status.card
; Port of cards/data/status/mental.json — revery (rect) + event (hex) cards.
; `magnetic.{recipe,duration}` has no op yet; expressed as flagged slot sets.

<card>
  ::dread>
    :data>
      @define>
        revery &aspect.type set

  ::test>
    :data>
      @define>
        event &aspect.type set
        1 &aspect.stack_joins set

  ::despair>
    :data>
      @define>
        event &aspect.type set
        1 &aspect.stack_joins set
        $recipe::despair_success &magnetic.recipe set   ; FLAG magnetic has no op — guessed slot set
        60000 &magnetic.duration set                     ; FLAG magnetic duration_ms

  ::strike>
    :data>
      @define>
        event &aspect.type set
        1 &aspect.stack_joins set
        $recipe::strike_success &magnetic.recipe set    ; FLAG magnetic has no op — guessed slot set
        60000 &magnetic.duration set                     ; FLAG magnetic duration_ms
