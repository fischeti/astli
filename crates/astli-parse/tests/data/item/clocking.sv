// A clocking block samples and drives signals with the skews its items give,
// and may declare properties. `default clocking name;` makes one declared
// elsewhere the default, and a modport names the blocks it may use.
interface bus_if (input logic clk);
  logic valid, ready;
  modport host (clocking cb, output trst_n);
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
