// A header that only names its ports, declared in the body, is listed
// once, as the body declares it; one the body leaves undeclared stays.
module old (a, b, .renamed(c), d);
  input a;
  output reg [3:0] b;
  inout c;
endmodule
