// A `.*` is hinted with the ports it connects, those no other connection
// beside it takes; a long list is cut, and the whole is its tooltip.

module core (
  input  logic clk_i,
  input  logic rst_ni,
  input  logic req_i,
  output logic gnt_o,
  input  logic [31:0] addr_i,
  input  logic [31:0] wdata_i,
  output logic [31:0] rdata_o
);
endmodule

module pair (input logic clk_i, input logic rst_ni);
endmodule

module top;
  logic clk_i, rst_ni, req_i, gnt_o;
  logic [31:0] addr_i, wdata_i, rdata_o;
  pair u_pair (.*);
  core u_core (.*);
  core u_beside (.clk_i, .req_i(1'b0), .*);
  // Nothing left to it.
  pair u_all (.clk_i, .rst_ni, .*);
endmodule
