// A name a header declares is found in the header and in each unit that
// includes it, each place once.
`include "a.svh"

module shared_a;
  shared_byte_t a = shared_inc(8'd1);
//                  ^
endmodule
