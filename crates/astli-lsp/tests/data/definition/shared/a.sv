// A definition another file declares is found there. A header two units
// include is read through each: what it declares is answered once, and a
// name each unit declares for it, in each.
localparam int Width = 8;
`include "a.svh"

module shared_a;
  shared_b inner ();
//^
endmodule
