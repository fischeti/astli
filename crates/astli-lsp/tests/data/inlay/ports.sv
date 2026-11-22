// A named connection is hinted with its port's direction and type, as the
// body the instance builds has it; a long type is cut.

interface bus_if;
  logic valid;
  modport host (output valid);
endinterface

module dev #(parameter int Width = 8) (
  input  logic             clk_i,
  output logic [Width-1:0] data_o,
  inout  wire              pad_io,
  input  struct packed { logic [7:0] first; logic [7:0] second; logic [7:0] third; } wide_i,
  bus_if.host              bus
);
endmodule

module top;
  logic clk_i;
  logic [3:0] data;
  wire pad;
  bus_if bus ();
  dev #(.Width(4)) u_dev (.clk_i, .data_o(data), .pad_io(pad), .wide_i('0), .bus);
endmodule
