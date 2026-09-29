// Only `always_ff` is sequential by its keyword: blocking assignments in
// `always_comb`, `always` and `initial` are another rule's business.
module other_blocks (
  input  logic a_i,
  output logic b_o
);
  logic c;
  always_comb b_o = a_i;
  always @(posedge a_i) c = a_i;
  initial c = 1'b0;
endmodule
