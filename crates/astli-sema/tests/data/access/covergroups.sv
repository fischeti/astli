// A covergroup reads what it samples, its conditions, and what its bins and
// options are; a `with` reads what it is over, and its expression.
module m;
  parameter int Max = 7;
  logic clk, en;
  logic [3:0] a;
  int q[$], n;
  covergroup cg @(posedge clk);
    option.goal = n;
    coverpoint a iff (en) {
      bins low = {[0:Max]} with (item > 1);
    }
  endgroup
  initial n = q.sum() with (item + a);
endmodule
