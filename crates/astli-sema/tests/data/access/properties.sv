// A property reads what it names and what an instance of it is passed, and
// a match writes its sequence's local variable. What an assertion's action
// block writes, the assertion drives.
module m;
  logic clk, a, b;
  int errors;
  sequence s (x);
    int n;
    (x, n = 0) ##1 (b, n++);
  endsequence
  assert property (@(posedge clk) s(a)) else errors++;
endmodule
