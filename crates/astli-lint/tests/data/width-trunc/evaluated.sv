// A width is compared wherever a body evaluates it: in the action of a
// concurrent assertion, the declarations of a block and of a loop, a loop's
// step, and a procedural continuous assignment.
module top (
  input logic c,
  input logic [7:0] a_i
);
  logic [3:0] n, m, k;

  always @(posedge c) assert property (c) else n = a_i;

  always_comb begin
    logic [3:0] t = a_i;
    m = t;
    for (logic [3:0] i = a_i; i < 4'd3; i = a_i) m = i;
  end

  initial force k = a_i;
endmodule
