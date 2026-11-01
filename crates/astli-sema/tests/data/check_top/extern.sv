// A definition declared only by `extern` builds as a leaf: its outputs
// drive what they connect, and its ports have the types its parameters give
// them.
typedef enum logic [1:0] {Idle, Busy} state_e;

extern module macro #(parameter int W = 8)
  (input logic clk, input state_e state, output logic [W-1:0] q);

module top (input logic clk);
  logic [7:0] q;
  logic [1:0] raw;
  macro u_a (.clk, .state(raw), .q);
  macro u_b (.clk, .state(Idle), .q);
endmodule
