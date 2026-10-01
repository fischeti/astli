// A parameter nothing reads, in the port list or the body; one read in a
// type, a dimension or a value is used, and a package's never reported.
package p;
  parameter int Spare = 1;
endpackage

module m #(parameter int Width = 8, parameter int Depth = 4, parameter type T = logic) ();
  localparam int Half = Width / 2;
  localparam int Never = 3;
  T [Half-1:0] x;
  logic [Width-1:0] y = '0;
endmodule
