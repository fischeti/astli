// A net's delay is the delay a statement takes, one value or a list of them
// in parentheses, and not only a single token.
module m;
  wire #pkg::DELAY a;
  wire [7:0] #1ns b = c;
  wire #(1, 2) d;
  assign #pkg::DELAY e = f;
endmodule
