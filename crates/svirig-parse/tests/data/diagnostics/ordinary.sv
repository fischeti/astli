module top #(parameter int W = 8) (input logic clk_i, output logic [W-1:0] q_o);
  always_ff @(posedge clk_i) q_o <= q_o + 1;
endmodule
