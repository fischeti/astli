// A signal read through an instance is as wide as that instance's body
// makes it, with its overrides; one read through a generate block, as wide
// as the block built, or the loop's iteration the index picks, makes it.
module sub #(parameter int W = 4) (output logic [W-1:0] y);
  assign y = '0;
endmodule

module top;
  logic [1:0] low;
  logic [7:0] byte_wide;
  logic [2:0] three;
  sub u_narrow ();
  sub #(.W(16)) u_wide ();
  sub #(.W(2)) u_fits ();
  for (genvar i = 1; i < 4; i++) begin : g_loop
    logic [i:0] bits;
    assign bits = '0;
  end
  // 4 bits into 2.
  assign low = u_narrow.y;
  // 16 bits into 8.
  assign byte_wide = u_wide.y;
  // 2 bits, which fit.
  assign low = u_fits.y;
  // 4 bits into 3, and 2, which fit.
  assign three = g_loop[3].bits;
  assign three = g_loop[1].bits;
endmodule
