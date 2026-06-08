<recipe>
  ::triple_corpus>
    @input>
      $card::corpus *slot.1.0.def_id eq if &slot.1.0 use
      $card::corpus *slot.1.1.def_id eq if &slot.1.1 claim
      $card::corpus *slot.1.2.def_id eq if &slot.1.2 claim

    @output>
      10 &sys.duration set
      rtl &slot.1.0.style set
      &slot.1.0 destroy

  ::dread_remover>
    @input>
      $card::dread *slot.1.0.def_id eq if &slot.1.0 use
      $card::dread *slot.1.1.def_id eq if &slot.1.1 claim

    @output>
      10 &sys.duration set
      ltr &slot.1.0.style set
      &slot.1.0 destroy
      &slot.1.1 destroy

  ::corpus_dust>
    @input>
      $card::corpus *root.def_id eq if &root use
      $card::dust *slot.1.0.def_id eq if &slot.1.0 claim
    
    @output>
      10 &sys.duration set
      ltr &slot.1.0.style set
      &root destroy
      $card::corpus_dim &root.owner.inventory create

  ::corpus_b_top>
    @input>
      $card::corpus *slot.1.0.def_id eq if &slot.1.0 use
      $card::corpus *slot.1.1.def_id eq if &slot.1.1 claim

    @output>
      10 &sys.duration set
      ltr &slot.1.0.style set
      &slot.1.0 destroy
      $card::corpus_dim &slot.1.0.owner.inventory create

  ::corpus_b_bottom>
    @input>
      $card::corpus *slot.2.0.def_id eq if &slot.2.0 use
      $card::corpus *slot.2.1.def_id eq if &slot.2.1 claim

    @output>
      10 &sys.duration set
      ltr &slot.2.0.style set
      &slot.2.0 destroy
      $card::corpus &slot.2.0.owner.inventory create

  ::despair_success>
    @input>
      $card::despair *root.def_id eq if &root use
      $card::dread *slot.1.0.def_id eq if &slot.1.0 claim

    @output>
      10 &sys.duration set
      ltr &slot.1.0.style set
      &slot.1.0 destroy
      &root destroy
      $card::corpus &slot.1.0.owner.inventory create

  ::despair_failure>
    @input>
      $card::despair *root.def_id eq if &root use

    @output>
      10 &sys.duration set
      ltr &root.style set
      &root destroy
      $card::dread &root.owner.inventory create

  ::strike_success>
    @input>
      $card::strike *root.def_id eq if &root use
      $card::corpus *slot.1.0.def_id eq if &slot.1.0 claim
      $card::corpus *slot.1.1.def_id eq if &slot.1.1 claim
      $card::corpus *slot.1.2.def_id eq if &slot.1.2 claim

    @output>
      10 &sys.duration set
      ltr &root.style set
      &slot.1.0 destroy
      &slot.1.1 destroy
      &slot.1.2 destroy
      &root destroy
      $card::corpus_dim &slot.1.0.owner.inventory create
      $card::corpus_dim &slot.1.0.owner.inventory create
      $card::corpus_dim &slot.1.0.owner.inventory create

  ::strike_failure>
    @input>
      $card::strike *root.def_id eq if &root use

    @output>
      10 &sys.duration set
      ltr &root.style set
      &root destroy
      $card::dread &root.owner.inventory create

  ::corpus_dim>
    @input>
      $card::corpus_dim *root.def_id eq if &root use

    @output>
      30 &sys.duration set
      ltr &root.style set
      &root destroy
      $card::corpus &root.owner.inventory create

  ::fleeting>
    @input>
      *root.aspect.fleeting 1 ge if &root borrow

    @output>
      *root.aspect.fleeting &var.0 set

      5 &sys.duration set
      *var.0 2 ge if 10 &sys.duration set
      *var.0 3 ge if 15 &sys.duration set
      *var.0 4 ge if 20 &sys.duration set

      rtl &root.style set
      &root destroy

  ; Stock read/write validation: a ROOT-ONLY recipe (operates on the root card,
  ; not a stack slot). While the tally's progress (per-card stock) is below 3,
  ; increment it. Reads `*root.aspect.progress` (decoded from the card's stock
  ; u32) and writes it back via `inc` (→ Effect::Stock → SetCardStock).
  ; Self-terminating at 3.
  ::prime>
    @input>
      $card::tally *root.def_id eq *root.aspect.progress 3 lt and if &root use

    @output>
      &root.aspect.progress inc
