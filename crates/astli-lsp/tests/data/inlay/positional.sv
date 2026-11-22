// A positional override or connection is hinted with the formal it lands
// on, unless it is written as the formal's name. A named one needs none.

module fifo #(parameter int Width = 8, parameter int Depth = 4) (
  input  logic             clk_i,
  input  logic             rst_ni,
  input  logic [Width-1:0] data_i,
  output logic [Width-1:0] data_o
);
endmodule

module top;
  logic clk_i, rst_n;
  logic [15:0] d, q;
  fifo #(16, 2) u_positional (clk_i, rst_n, d, q);
  // Positional overrides, named connections.
  fifo #(16) u_mixed (.clk_i, .rst_ni(rst_n), .data_i(d), .data_o(q));
  // Named overrides, positional connections, one left empty.
  fifo #(.Width(16)) u_other (clk_i, , d[15:0], q);
  // Past the last formal, nothing.
  fifo u_extra (clk_i, rst_n, d, q, d);
endmodule
