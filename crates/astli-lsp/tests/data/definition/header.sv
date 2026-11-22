// An `include goes to the header it read, and what a header declares is
// found through the unit that includes it.
`include "header.svh"
//^        ^

module header;
  state_t state;
//^
  always_comb state = next(state);
//                    ^
endmodule
