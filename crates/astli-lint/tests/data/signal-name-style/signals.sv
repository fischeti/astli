// A net, variable or port of a design element is lower_snake_case, in a
// function inside one too. A class's variables and a package's are not
// signals.
module signals (
  input logic clk_i,
  input logic DataIn
);
  logic valid_q, ReadyQ;
  wire  BUS_W;
  bit   ReadyIsStable;
  function automatic void f();
    logic TmpVar;
  endfunction
endmodule

package pkg;
  int GlobalCount;
endpackage

class item;
  int SomeField;
endclass
