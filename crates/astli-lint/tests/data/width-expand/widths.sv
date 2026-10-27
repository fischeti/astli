// A value extended to a wider target, port or operand, as each body builds
// it; and what takes the width it is given without a word.
module sub #(parameter int W = 8) (
  input  logic [W-1:0] a_i,
  output logic [W-1:0] y_o
);
  assign y_o = a_i;
endmodule

module top (
  input  logic [7:0]  a_i,
  input  logic [3:0]  b_i,
  input  logic        c_i,
  output logic [8:0]  sum_o,
  output logic [15:0] wide_o,
  output logic [15:0] port_o
);
  typedef enum logic [1:0] {Idle, Busy} state_e;
  logic [3:0] state;
  logic       less;
  logic [15:0] shifted;

  // Extended.
  assign wide_o = a_i;
  // `+` keeps its carry, into one bit more.
  assign sum_o = a_i + a_i;
  // A constant takes the width it is assigned to; one written with a size
  // has that size.
  assign state = c_i ? Idle : Busy;
  assign shifted = 4'h3;
  // A shift left fills what it is taken at.
  assign port_o = c_i << b_i;
  // The narrower operand of a comparison.
  assign less = b_i < a_i;
  // What a time scales reaches all of its 64 bits.
  localparam time Period = 10ns;
  time half;
  assign half = Period * b_i / 2;
  // A port connected wider than its connection, in the body that builds it.
  sub #(.W(16)) u_wide (.a_i(a_i), .y_o());
  sub u_same (.a_i(a_i), .y_o());
endmodule
