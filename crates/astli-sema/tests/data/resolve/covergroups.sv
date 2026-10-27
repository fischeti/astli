// What a cover point samples is found around the covergroup, so a label
// does not hide the variable of its name; what a cross crosses, and what a
// bin takes, is a cover point by label, or by the variable it alone
// samples. A `with` finds its iterator; `option` is no name. A cover point
// is a member of its covergroup's instance.
module m;
  parameter int Max = 7;
  logic clk;
  logic [3:0] a, b;
  int q[$];
  covergroup cg (int lo) @(posedge clk);
    option.per_instance = 1;
    a: coverpoint a {
      bins low = {[lo:Max]};
      bins odd = a with (item % 2 == 1);
    }
    coverpoint b;
    ab: cross a, b, nope;
  endgroup
  cg c = new(0);
  initial begin
    int s = q.find(x) with (x > c.a.get_coverage());
  end
endmodule
