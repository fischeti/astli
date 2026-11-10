// What is not lowered keeps the names it spells: a stream's slice of an
// array.
module m;
  initial b = {<< 8 {a with [0 +: n]}};
endmodule
