// A `specparam` is a parameter for timing, allowed in a module outside
// `specify` too, with a packed range if it has one.
module m;
  specparam delay = 50;
  specparam [3:0] rise = 1, fall = 2;
endmodule
