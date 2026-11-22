// A header two units include is read through the first of them: in this
// one, `Size` is a constant.
localparam int Size = 8;
`include "a.svh"

module shared_a;
endmodule
