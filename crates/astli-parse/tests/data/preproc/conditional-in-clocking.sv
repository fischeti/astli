// A branch inside a clocking block holds what the block does: signals with
// their skews, and declarations.
module m;
  clocking cb @(posedge clk);
`ifdef X
    input #1 b;
`else
    output negedge c;
    property p; b |-> c; endproperty
`endif
  endclocking
endmodule
