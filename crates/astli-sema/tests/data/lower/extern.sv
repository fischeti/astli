// An `extern` declaration is a definition with its ports and parameters
// and no body, ANSI or not, with the imports its header makes; a body
// declared in the same file stands in for it.
package cfg_pkg;
  localparam int W = 8;
endpackage

extern module sram import cfg_pkg::*; #(parameter int Words = 256)
  (input logic clk, input logic [W-1:0] d, output logic [W-1:0] q);
extern module old (a, b);
extern interface bus_if (input logic clk);
extern module model (input logic i, output logic o);

module model (input logic i, output logic o);
  assign o = i;
endmodule
