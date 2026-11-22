// A macro call says the `define it read and what this call expands to: a
// multi-line one laid out, as the formatter lays out module items. A name a
// macro wrote is found where the `define spells it, once for each
// expansion, and what it refers to is said once. A built-in says what it
// stands for.

`include "macros.svh"

// Reads the source into a target.
`define READ(target) assign target = source;
//           ^                       ^

`define FLOP(q, d) \
  always_ff @(posedge clk) begin \
    q <= d; \
  end

`define WIDTH 8

module top;
  logic clk;
  logic source;
  logic x, y;
  logic [`WIDTH-1:0] wide;
//       ^
  `READ(x)
//^     ^
  `READ(y)
  `FLOP(y, x)
//^
  localparam int Line = `__LINE__;
//                      ^
  assign shared = 1'b1;
//       ^
endmodule
