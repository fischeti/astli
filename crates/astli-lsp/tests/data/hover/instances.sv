// An instance, and the definition it names, say the definition as
// written and what its overrides evaluate to in the body it builds, for
// each body of the definition around it. A formal an override or a
// connection names, or a positional one is given to, is shown as its
// declaration is.

// Adds two words.
module adder #(parameter int W = 8, parameter int Lanes = 1) (
  input  logic [W-1:0] a,
  input  logic [W-1:0] b,
  output logic [W:0]   sum // One wider.
);
  assign sum = a + b;
endmodule

module wrap #(parameter int N = 2) (input logic [N-1:0] x);
  adder #(.W(N * 2)) u_inner (.a(), .b(), .sum());
//^                  ^
endmodule

module top;
  logic [15:0] a, b;
  logic [16:0] sum;
  adder #(.W(16), .Lanes(2)) u_named (.a, .b(b), .sum(sum));
//^        ^                 ^         ^   ^
  adder #(16) u_positional (a, b, sum);
//        ^                 ^
  adder u_default (.a(a[7:0]), .b(b[7:0]), .sum(sum[8:0]));
//      ^
  wrap #(.N(4)) u_wrap (.x());
  wrap u_wrap_default (.x());
  and u_gate (sum[0], a[0], b[0]);
//^   ^

  nowhere u_nowhere (.p(a));
//^       ^           ^
endmodule
