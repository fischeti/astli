// An `extern` design unit is its header alone, and the definition may take
// its ports with `(.*)`. An interface may declare a task each module that
// exports it defines, `forkjoin` to run them all at once.
extern module counter #(parameter int W = 8) (input logic clk, output logic [W-1:0] q);
extern macromodule legacy (a, b);
extern interface bus_if (input logic clk);
extern program test;

module counter (.*);
endmodule

interface bus_if (input logic clk);
  extern forkjoin task reset (input int cycles);
endinterface
