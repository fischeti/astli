// A header's declarations are the unit's, and a name the file uses is
// found in them; the header's own places are marked in it.
`include "header.svh"

module top;
  state_t state;
//^
  always_comb state = next(state);
//                    ^    ^
endmodule
