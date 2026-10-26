// A property's formals and local variables are found in it, its instances
// name it, and a label names its assertion. A name an assertion reads and
// nothing declares is undeclared, as anywhere else.
module m;
  logic clk, a, b;
  sequence s (x);
    int n;
    (x, n = 0) ##1 b;
  endsequence
  property p (x, y = s(x));
    @(posedge clk) x |-> y;
  endproperty
  a_p: assert property (p(a));
  assert property (@(posedge clk) s.triggered |-> nope);
  initial $assertoff(0, a_p);
endmodule
