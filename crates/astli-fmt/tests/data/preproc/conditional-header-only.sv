// A branch holding only a statement's header, its body after the `endif`,
// is left a statement without one: what follows is the block's next statement.
module m;
  initial begin
`ifdef A
        if (a)
`endif
`ifdef B
`endif
  y = 2;
`ifdef C
  if (c) x = 1;
  else
`endif
  y = 3;
  end
endmodule
