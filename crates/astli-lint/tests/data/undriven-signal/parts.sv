// Bits a read or an output needs that no write reaches, as the
// definition's defaults evaluate the selects; a part sema cannot evaluate
// may be any.
module m #(parameter int W = 8) (
  input  logic [1:0]   sel_i,
  input  logic         d_i,
  output logic [W-1:0] half_o,
  output logic [3:0]   whole_o,
  output logic         read_o
);
  logic [W-1:0] low, split, looped, indexed;
  logic [0:3] up;
  logic [3:0] gaps;

  assign half_o[W/2-1:0] = '0;
  assign whole_o[1:0] = '0;
  assign whole_o[3:2] = '1;

  // Only the bits written are read.
  assign low[3:0] = '0;
  assign split[W-1:4] = '1;
  assign up[0:1] = '0;
  assign gaps[0] = d_i;
  assign gaps[2] = d_i;

  for (genvar i = 0; i < W; i++) begin : g_bit
    assign looped[i] = d_i;
  end
  always_comb indexed[sel_i] = d_i;

  assign read_o = ^{low[3:0], split, up, gaps, looped, indexed};
endmodule

// Named by the members and elements no write reaches.
module n (output logic [7:0] sum_o);
  typedef struct packed {
    logic [3:0] tag;
    logic       valid;
    logic [2:0] data;
  } entry_t;
  entry_t entry;
  logic [1:0][3:0] nibbles;
  logic [3:0] words [3];

  assign entry.tag = '0;
  assign entry.data[0] = 1'b1;
  assign nibbles[0] = '0;
  assign words[0] = '0;
  assign sum_o = {entry, nibbles} + words[0] + words[2];
endmodule
