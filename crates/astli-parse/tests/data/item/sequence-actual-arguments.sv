// An actual argument to a sequence or property instance may be a sequence,
// `ready ##1 go`, or an event expression joined by `or` or guarded by `iff`.
module m;
  logic clk, x, ready, go;
  sequence s(sequence a);
    a;
  endsequence
  sequence e(event ev);
    @(ev) x;
  endsequence
  cover property (@(posedge clk) s(ready ##1 go) ##1 1);
  cover property (e(posedge clk or negedge clk));
  cover property (e(posedge clk iff x));
endmodule
