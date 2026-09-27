// An immediate assertion checks its condition once; one that names a
// property or a sequence is concurrent, and left to the fallback. A deferred
// one may stand among items, labelled or not, and runs statements all the
// same. What runs when the condition holds may be left out before `else`.
module m;
  aw_id : assert final (a === b) else $fatal(1, "id");
  assert #0 (c);
  assert property (@(posedge clk) a |-> b);
  initial begin
    assert (y) else $error("y");
    assume (z) $display("ok"); else $error("z");
    cover (w);
  end
endmodule
