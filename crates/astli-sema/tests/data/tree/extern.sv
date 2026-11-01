// An instance of a definition declared only by `extern` is a leaf, its
// body compiled apart; one whose body a file declares too is built from it.
extern module sram #(parameter int Words = 256) (input logic clk);
extern module model (input logic clk);

module model (input logic clk);
  leaf u_leaf (.clk);
endmodule

module leaf (input logic clk);
endmodule

module top (input logic clk);
  sram u_sram (.clk);
  sram #(.Words(64)) u_small (.clk);
  model u_model (.clk);
endmodule
