// A parameter has a type; a range or `signed` alone is none. One whose value
// is a string may go without, as Verilog wrote them, and a type parameter
// is a type.
module params #(
  parameter int Width = 8,
  parameter Depth = 4,
  parameter [7:0] Mask = 8'hff,
  parameter Name = "fifo",
  parameter type data_t = logic
) ();
  localparam int unsigned Half = Width / 2;
  localparam Double = 2 * Width;
  localparam logic Flag = 1'b1;
endmodule
