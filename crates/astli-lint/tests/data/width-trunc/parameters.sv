// A parameter's default is sized against its type where nothing overrides
// it, and an override against the parameter's type in the body it builds.
module sub #(
  parameter logic [3:0] W = 8'hff,
  parameter logic [3:0] V = 9'h1ff
) ();
endmodule

module top;
  localparam logic [3:0] Low = 9'h1ff;
  sub #(.V(8'hf0)) u_sub ();
endmodule
