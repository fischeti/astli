// A comment at the end of a line lines up with the others of its table, after
// every other column. A line comment right below one, in the same column,
// continues it and moves with it. A comment too long to line up stays one
// space after the code, and the rest of its row stays aligned.
module m;
  logic [31:0] pc_if; // program counter
  logic valid;
  logic [1:0] mode;  // two bits, which
                     // take two lines
  logic this_one_is_long; // sets the column
  logic x; // a comment long enough that lining it up would take the line past the width
  logic b; /* a block comment */

  assign valid = 1'b1; // outside a table
                       // still continued

  mod u_mod (
    .clk_i,          // clock
    .d_i(d),         // data
    .a_longer_name_o(q) // out
  );
endmodule
