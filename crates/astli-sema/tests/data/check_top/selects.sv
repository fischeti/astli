// Writes to parts of a variable clash where the bits they reach overlap, as
// each place builds them; a part sema cannot evaluate clashes only with a
// write to all of it.
typedef struct packed {
  logic [3:0] hi;
  logic [3:0] lo;
} pair_t;

module top #(parameter int N = 4) (input logic d, input logic [1:0] sel);
  logic [N-1:0] disjoint, overlap, looped, shifted, unknown;
  logic [7:0] words [2];
  pair_t pair, same;

  assign disjoint[1:0] = {d, d};
  assign disjoint[3:2] = {d, d};

  assign overlap[2:0] = {d, d, d};
  assign overlap[3:2] = {d, d};

  for (genvar i = 0; i < N; i++) begin : g_bit
    assign looped[i] = d;
  end

  // Each iteration writes the bit the next one does.
  for (genvar i = 0; i < N - 1; i++) begin : g_pair
    assign shifted[i+:2] = {d, d};
  end

  always_comb unknown[sel] = d;
  assign unknown[3] = d;

  assign words[0] = '0;
  assign words[1][3:0] = '0;
  assign words[1][7:4] = '1;

  assign pair.hi = '0;
  assign pair.lo = '1;
  assign same.lo = '0;
  always_comb same[1:0] = '1;
endmodule
