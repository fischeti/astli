// A name a macro writes is spelled in the `define's body, so the place
// there stands for a use in each expansion; a name an argument gives, at
// the call. A formal of the macro, in its body, stands for no name.
`define READ(target) assign target = source;
//                          ^        ^

module top;
  logic source;
  logic x, y;
  `READ(x)
//      ^
  `READ(y)
endmodule
