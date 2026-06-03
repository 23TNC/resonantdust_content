<recipe>
  ::cut_tree>
    @input>
      *slot.0.0.aspect.wood 1 ge if &slot.0.0 use
      *slot.1.0.aspect.corpus_lit 1 ge if &slot.1.0 claim
      $card::axe *slot.1.0.owner.slot.1.0.def_id eq if &slot.1.0.owner.slot.1.0 share

    @output>
      10 &sys.duration set
      ltr &slot.1.0.style set
      &slot.1.0 destroy
      &slot.0.0.aspect.wood dec
      $card::corpus_dim &slot.1.0.owner.inventory create
      $card::log &slot.1.0.owner.inventory create
      $card::blueprint_nd_furnace &slot.1.0.owner.blueprint set   ; FLAG blueprint.unlock has no op yet — guessed as a slot set

  ::stick>
    @input>
      *slot.2.0.aspect.wood 2 ge if &slot.2.0 use
      *slot.2.1.aspect.corpus_lit 1 ge if &slot.2.1 claim
      $card::axe *slot.2.0.owner.slot.1.0.def_id eq if &slot.2.0.owner.slot.1.0 share

    @output>
      10 &sys.duration set
      ltr &slot.2.1.style set
      &slot.2.0 destroy
      &slot.2.1 destroy
      $card::corpus_dim &slot.2.1.owner.inventory create
      *slot.2.0.aspect.wood &var.0 set
      *var.0 2 gt if $card::stick &slot.2.1.owner.inventory create
      $card::stick &slot.2.1.owner.inventory create
      $card::stick &slot.2.1.owner.inventory create

  ::break_rock>
    @input>
      *slot.0.0.aspect.stone 1 ge if &slot.0.0 use
      $card::corpus *slot.1.0.def_id eq if &slot.1.0 claim
      $card::pickaxe *slot.1.0.owner.slot.1.0.def_id eq if &slot.1.0.owner.slot.1.0 share

    @output>
      10 &sys.duration set
      ltr &slot.1.0.style set
      &slot.1.0 destroy
      &slot.0.0.aspect.stone dec
      $card::corpus_dim &slot.1.0.owner.inventory create
      $card::stone &slot.1.0.owner.inventory create
