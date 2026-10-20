// A parameter's value, and one a `#(...)` overrides it with, may be minimum,
// typical and maximum values. A call's argument may not.
module m #(parameter P = 1:2:3) ();
  parameter Q = 4:5:6, R = 7;
  specparam tRise = 1:2:3;
  n #(1:2:3) u0 ();
  n #(.P(1:2:3)) u1 ();
endmodule
