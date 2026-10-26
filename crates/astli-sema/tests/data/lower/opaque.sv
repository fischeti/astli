// What is not lowered keeps the names it spells: a class body, a clocking
// block, a covergroup, a modport.
interface i;
  logic a, b;
  modport mp (input a, output b);
  clocking cb @(posedge clk); input a; endclocking
endinterface

module m;
  class C extends B;
    int x;
  endclass
  covergroup cg @(posedge clk);
    coverpoint a;
  endgroup
  always_comb s = arr.sum() with (item * 2);
endmodule
