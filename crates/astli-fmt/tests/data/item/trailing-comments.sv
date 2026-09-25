// A comment at the end of a line lines up with the others of its table, after
// every other column. A line comment right below one, in the same column,
// continues it and moves with it. A comment too long to line up stays one
// space after the code, and the rest of its row stays aligned. A run of items
// of a line each, such as calls, is a table of its comments alone, which an
// item with a body ends.
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
  assign mode = 2'b01; // one
  assign pc_if = '0;  // two

  assign b = x; // before a block
  always_comb begin
    c = 1; // inside
    d = 22; // inside, too
  end
  assign e = y & z; // after a block
  initial begin
    $display("a"); // a call
    $display("bb"); // another
  end

  mod u_mod (
    .clk_i,          // clock
    .d_i(d),         // data
    .a_longer_name_o(q) // out
  );
endmodule
