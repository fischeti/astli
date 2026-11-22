// A name used is a use and what it refers to, and its declaration is a
// declaration; one nothing declares is undeclared. A member of a struct is
// found through the type of what it is a member of, at the field declaring
// it.
package pkg;
  localparam int unsigned Width = 8;
endpackage

module top (input logic clk);
  typedef struct packed {
    logic [pkg::Width-1:0] data;
//                         ^
    logic valid;
  } beat_t;
  beat_t beat;
//^      ^
  logic ready;
  assign ready = beat.valid & missing;
//       ^       ^    ^       ^
  always_ff @(posedge clk) beat.data <= '0;
//                    ^         ^
  logic [pkg::Width-1:0] copy;
//       ^    ^
endmodule
