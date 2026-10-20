// `wait_order` runs what follows if its events come in turn, and what
// follows `else` if they do not; either may be left out.
module m;
  event e1, e2, e3;
  initial begin
    wait_order (e1, e2, e3) $display("in order"); else $error("out of order");
    wait_order (e1, top.u.e2);
    wait_order (e1, e2) else $error("out of order");
  end
endmodule
