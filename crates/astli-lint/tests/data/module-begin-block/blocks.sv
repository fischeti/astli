// A `begin` directly in a module is reported, inside an `ifdef too; one that
// is a generate block's body, or procedural, is not.
module blocks;
  begin
  end
`ifdef SIM
  begin : named
  end
`endif
  if (1) begin : gen_a
  end
  initial begin
  end
endmodule
