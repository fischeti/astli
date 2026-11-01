// An instance of a definition declared only by `extern`, as a hard macro
// linked later is, is checked against its ports and parameters; and a body
// stands in for an extern declaration of it, which is no second definition.
extern module sram #(parameter int Words = 256)
  (input logic clk, input logic [7:0] d, output logic [7:0] q);
extern module model (input logic i, output logic o);

module model (input logic i, output logic o);
  assign o = i;
endmodule

module top (input logic clk);
  logic [7:0] d, q;
  logic o;
  sram u_ok (.clk, .d, .q);
  sram #(.Depth(16)) u_param (.clk, .d, .q(), .ready());
  sram u_many (clk, d, q, o);
  sram u_twice (.clk, .clk(clk), .d, .q());
  model u_model (.i(clk), .o, .x());
endmodule
