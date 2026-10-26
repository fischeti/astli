// What a modport or a clocking block lists is a name in the scope around
// it, and one nothing declares is undeclared. `cb.name` is what the block
// names it for; a name the block does not list is unknown.
interface bus_if (input logic clk);
  logic valid, ready;
  clocking cb @(posedge clk);
    output valid;
    input ready, nope;
  endclocking
  modport host (output valid, input ready, missing);
  default clocking cb;
endinterface

module m (input logic clk);
  logic a, b;
  clocking cb @(posedge clk);
    input a;
    output b;
    input first = a;
  endclocking
  initial begin
    @(cb);
    cb.b <= cb.a | cb.first;
    if (cb.other) ;
  end
endmodule
