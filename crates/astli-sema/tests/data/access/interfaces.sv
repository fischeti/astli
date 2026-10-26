// A modport's users may read its inputs and drive its outputs, for all the
// analysis knows. A clocking block reads its inputs on every event, and may
// drive its outputs; a procedure that drives one through the block writes
// the signal.
interface bus_if (input logic clk);
  logic valid, ready;
  modport host (output valid, input ready);
endinterface

module m (input logic clk);
  logic a, b, c;
  clocking cb @(posedge clk);
    input a;
    output b;
    inout c;
  endclocking
  initial cb.b <= cb.a;
endmodule
