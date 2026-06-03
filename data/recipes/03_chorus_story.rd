<recipe>
  ::chorus_attune>
    @input>
      $card::alter *slot.0.0.def_id eq if &slot.0.0 use
      *slot.1.0.aspect.corpse 1 ge if &slot.1.0 claim

    @output>
      10 &sys.duration set
      ltr &slot.1.0.style set
      $faction.chorus &slot.0.0.owner.aspect.faction set
      &slot.1.0 destroy
