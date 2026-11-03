// An indexed part-select takes at least one bit, in what the tops build;
// at a definition's defaults a width may be 0 that no instance leaves.
module sub #(parameter int W = 0) (input logic [7:0] x, output logic [7:0] y);
  assign y[4+:W] = x[1+:W];
endmodule

module top (input logic [7:0] x, output logic [7:0] y, z);
  sub #(.W(0)) u_empty (.x, .y);
  sub #(.W(2)) u_wide (.x, .y(z));
endmodule
