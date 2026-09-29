// A parameter or localparam is CamelCase or ALL_CAPS, in a header or a body.
// CamelCase starts with a capital or a digit and may end in `_` and digits.
// A type parameter is named as a type, so this rule leaves it alone.
module params #(
  parameter int  Width     = 8,
  parameter int  NUM_PORTS = 2,
  parameter int  numLanes  = 4,
  int unsigned   Depth_    = 4,
  parameter type data_t    = logic
) ();
  localparam int unsigned AXI4Lite     = 1;
  localparam int unsigned InitHash_256 = 1;
  localparam int unsigned Sha2_256Hash = 1;
  localparam int unsigned tCK = 1, Ok = 2;
  localparam type         word_t       = logic [31:0];
endmodule
