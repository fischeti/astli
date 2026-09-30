// What is not lowered keeps the names it spells: a class body, a property,
// a covergroup, a concurrent assertion, a modport.
interface i;
  logic a, b;
  modport mp (input a, output b);
  clocking cb @(posedge clk); input a; endclocking
endinterface

module m;
  class C extends B;
    int x;
  endclass
  property p;
    @(posedge clk) a |-> b;
  endproperty
  assert property (p);
  label: assert property (@(posedge clk) a);
  covergroup cg @(posedge clk);
    coverpoint a;
  endgroup
  always_comb s = arr.sum() with (item * 2);
endmodule
