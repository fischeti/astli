// A negative literal bound is reported; a bound computed by subtraction, or
// a negative value elsewhere, is not.
module dims;
  logic [-1:0] a;
  logic [7:-4] b;
  logic [Width-1:0] c;
  logic d [0:-2];
  initial c = -1;
endmodule
