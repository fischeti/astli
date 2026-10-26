// What is not lowered keeps the names it spells: a class body, a
// covergroup, a `with` clause.
module m;
  class C extends B;
    int x;
  endclass
  covergroup cg @(posedge clk);
    coverpoint a;
  endgroup
  always_comb s = arr.sum() with (item * 2);
endmodule
