// A macro is found from its `define and from a call: each call, one
// another macro's body writes too, once however often that body is
// expanded.
`define MACROS_ONE 1
//      ^
`define MACROS_TWO (`MACROS_ONE + `MACROS_ONE)

module macros;
  localparam int A = `MACROS_ONE;
//                    ^
  localparam int B = `MACROS_TWO;
  localparam int C = `MACROS_TWO;
endmodule
