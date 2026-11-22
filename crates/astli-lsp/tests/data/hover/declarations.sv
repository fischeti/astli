// A declaration and a use of it say its kind, its type as elaborated with
// its width where the type does not show it, and the declaration as
// written, with the comment directly above it or after it on its line. Of
// several names, the one hovered is shown. A marker line is a comment, so
// a blank line keeps it from the declaration below.

typedef enum logic [1:0] {
  // Nothing to do.
  IDLE,
  BUSY = 2'd2 // Working.
} state_e;

typedef struct packed {
  logic [3:0] tag;
  logic valid;
} entry_t;

module counter (
//     ^

  // The clock, rising edge.
  input  logic       clk_i,
  input  logic       rst_ni, // Active low.
  output logic [7:0] count_o
);
  wire w;
//     ^

  // The next count and the one before.
  logic [7:0] next, last;
//                  ^

  state_e state;
//^       ^

  entry_t entry;
//        ^

  /* Counts up
     by one. */
  function automatic logic [7:0] bump(input logic [7:0] value);
//                               ^
    return value + 8'd1;
  endfunction

  task automatic idle();
//               ^
  endtask

  assign next = bump(count_o);
//       ^      ^    ^
  assign w = clk_i & rst_ni;
//           ^       ^
  assign state = IDLE;
//               ^
  assign state = BUSY;
//               ^
  assign implicit = w;
//       ^

  for (genvar i = 0; i < 2; i++) begin : g_each
//            ^
  end
endmodule
