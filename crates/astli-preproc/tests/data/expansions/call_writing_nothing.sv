// A call that writes nothing leaves no token, and is still listed.
`define NOTHING
`define EMPTY(x)
module m; `NOTHING `EMPTY(a + b) endmodule
