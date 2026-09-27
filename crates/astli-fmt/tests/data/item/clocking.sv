// A clocking block is laid out as a module is, an item on each line.
interface bus_if (input logic clk);
  logic valid, ready;
  logic [7:0] data;
  default clocking @(posedge clk_i); endclocking
  clocking cb @(posedge clk);
    default input #1step output #2;
    input  ready;
    output valid,  data;
    input negedge #1 x = top.y;
    property p; valid |-> ready; endproperty
  endclocking : cb
  default clocking cb;
  global clocking g @(posedge clk); endclocking
endinterface
