// A member read through an instance has the type its body gives it, with
// the overrides of that instance; one read through a generate block, the
// type it has in the block built, or in the loop's iteration the index
// picks. A block not built, or an iteration past the loop's end, gives
// none.
module sub #(parameter int W = 4) (output logic [W-1:0] y);
  if (W > 2) begin : g_wide
    logic [W:0] more;
  end
  assign y = '0;
endmodule

module m #(parameter int N = 2);
  logic [7:0] a, b, c, d, e, f;
  sub u_narrow ();
  sub #(.W(8)) u_wide [2] ();
  sub #(.W(1)) u_tiny ();
  for (genvar i = 0; i < N; i++) begin : g_loop
    logic [i:0] bits;
  end
  assign a = u_narrow.y;
  assign b = u_wide[1].y;
  assign c = u_wide[0].g_wide.more;
  assign d = g_loop[1].bits;
  assign e = g_loop[2].bits;
  assign f = u_tiny.g_wide.more;
endmodule
