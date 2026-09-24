// Nothing enclosing a ragged region can balance. What the region costs is the
// construct around it: the module after it is reached intact.
module m;
`ifdef SYN
  if (a) begin
`else
  if (b) begin
`endif
  end
endmodule
module n;
endmodule
