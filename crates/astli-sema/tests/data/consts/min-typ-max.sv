// A parameter given minimum, typical and maximum values takes the typical
// one, as a tool does unless told otherwise, in parentheses too.
module m #(parameter int P = 1:2:3) ();
  localparam int Q = P + (4:5:6);
endmodule
