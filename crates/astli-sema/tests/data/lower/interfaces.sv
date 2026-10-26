// A modport lists what its users read and drive, each entry with the
// direction of the one before it unless it writes its own; `.name(expr)`
// names an expression, and `clocking cb` a clocking block. A clocking block
// samples its inputs and drives its outputs on its event, with the skews
// its items write; `input output` is `inout`, and `name = expr` names an
// expression. `default clocking cb;` names one declared apart.
interface bus_if (input logic clk);
  logic valid, ready;
  logic [7:0] data;
  clocking cb @(posedge clk);
    default input #1step output #2;
    input ready;
    output negedge #1 valid, data;
    input output data;
    input lsb = data[0];
    property p_hold;
      valid |=> data;
    endproperty
  endclocking
  modport host (output valid, data, input ready, .lsb(data[0]), clocking cb);
  modport dev (input valid, data, output ready);
  default clocking cb;
  global clocking @(posedge clk); endclocking
endinterface
