// Writes to parts of a variable clash where the bits they reach overlap at
// the definition's defaults; a part sema cannot evaluate clashes only with
// a write to all of it.
typedef struct packed {
  logic [3:0] hi;
  logic [3:0] lo;
} pair_t;

module m #(parameter int W = 4) (input logic d_i, input logic [1:0] sel_i);
  logic [W-1:0] disjoint, overlap, looped, indexed;
  pair_t pair, same;

  assign disjoint[W/2-1:0] = '0;
  assign disjoint[W-1:W/2] = '1;

  assign overlap[2:0] = '0;
  always_comb overlap[3] = d_i;
  always_comb overlap[2] = d_i;

  for (genvar i = 0; i < W; i++) begin : g_bit
    assign looped[i] = d_i;
  end
  assign looped[0] = d_i;

  always_comb indexed[sel_i] = d_i;
  assign indexed[3] = d_i;

  assign pair.hi = '0;
  assign pair.lo = '1;
  assign same.lo = '0;
  always_comb same[1:0] = '1;
endmodule
