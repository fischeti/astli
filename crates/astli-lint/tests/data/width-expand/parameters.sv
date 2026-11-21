// An override is sized against the parameter's type in the body it builds,
// and a default against its own where nothing overrides it.
module sub #(
  parameter int N = 4'd2,
  parameter int M = 1
) ();
endmodule

module top;
  sub #(.M(4'd3)) u_sub ();
endmodule
