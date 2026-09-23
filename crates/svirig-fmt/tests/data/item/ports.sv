// Consecutive ports line up in three columns: the direction, the type and the
// name. A port without a type leaves its column empty.
module m #(
  parameter int unsigned Width = 8
) (
  input clk_i,
  input  logic rst_ni,
  input logic [Width-1:0] d_i,
  output logic [Width-1:0] q_o,  // registered
  output tlul_pkg::tl_d2h_t tl_o
);
endmodule
module n (
  axi_if.slave  bus,
  input  x
);
endmodule
