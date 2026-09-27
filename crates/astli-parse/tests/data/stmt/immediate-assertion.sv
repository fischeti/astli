// An immediate assertion checks its condition once; one that names a
// property or a sequence is concurrent, and left to the fallback. A deferred
// one may stand among items, labelled or not, and runs statements all the
// same. What runs when the condition holds may be left out before `else`;
// a `cover` has no `else`, so one after it is an `if`'s.
module m;
  aw_id : assert final (a === b) else $fatal(1, "id");
  assert #0 (c);
  assert property (@(posedge clk) a |-> b);
  initial begin
    assert (y) else $error("y");
    assume (z) $display("ok"); else $error("z");
    cover (w);
    if (a) cover (w) $display("hit"); else $display("not a cover's");
  end
endmodule
