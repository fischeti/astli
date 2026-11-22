// With the restriction rules on, several fixes in one file. Two insert
// at the same place, so fixing all of them leaves one for the next run.
module restricted (
  input  logic a,
  input  logic b,
  input  logic c,
  input  logic d,
  output logic x,
  output logic y
);
`ifdef FAST
  assign x = a | b & c;
`endif
  generate
    if (1) begin : gen_y
      assign y = a == b && c || d;
    end
  endgenerate
endmodule
