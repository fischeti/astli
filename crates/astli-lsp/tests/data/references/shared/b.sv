// The other unit that includes the header.
`include "a.svh"

module shared_b;
  shared_byte_t b;
//^
endmodule
