// An instance names its definition, and each named override and
// connection names a formal of it; a positional one is given to a formal by
// its place, past the last to none. `.name` names a port and uses the name
// where the instance stands. An instance of a definition no file declares
// has no formals. Instances whose parameters are equal share a body.
module sub #(parameter int W = 1, parameter int D = 2) (
  input logic clk,
  input logic [W-1:0] a,
  output logic [W-1:0] y
);
endmodule

module top;
  logic clk;
  logic [3:0] a, y;
  sub #(.W(4), .Missing(1)) u_named (.clk, .a(a), .y(y));
//^      ^      ^           ^         ^     ^ ^
  sub #(4, 2, 7) u_positional (clk, a, y, a);
//      ^     ^                ^       ^  ^
  sub u_default (.clk, .a(a[0]), .y(y[0]));
//^
  nowhere u_nowhere (.p(a), a);
//^                   ^     ^
endmodule
