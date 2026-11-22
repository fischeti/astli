// A macro call goes to the name its `define defines, which goes to itself.
// A name a macro wrote goes, from the `define's body, to what it is in each
// expansion, and from an argument, to what it is at that call; a formal, in
// its body, is no name. A built-in macro, and one never defined, go nowhere.
`define READ(target) assign target = source;
//      ^                   ^        ^

module macros_a;
  logic source, x;
  `READ(x)
//^ ^   ^
endmodule

module macros_b;
  logic source, y;
  `READ(y)
  localparam int Line = `__LINE__;
//                      ^
  localparam int Nope = `NOPE;
//                       ^
endmodule
