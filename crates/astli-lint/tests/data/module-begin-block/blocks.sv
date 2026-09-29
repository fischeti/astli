// A `begin` directly in a module is reported, inside an `ifdef or after a
// label too; one that is a generate block's body, or procedural, is not.
module blocks;
  begin
  end
`ifdef SIM
  begin : named
  end
`endif
  labelled : begin
  end
  if (1) begin : gen_a
  end
  initial begin
  end
endmodule
