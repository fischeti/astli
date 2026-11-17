// A reset given a value is given a constant or another reset; what logic
// drives glitches. A read inside such a value is the write's finding.
module derived (
  input logic clk_i,
  input logic rst_ni,
  input logic clear_i
);
  logic rst_gated_ni;
  logic rst_q_ni;
  logic rst_clk_ni = clk_i;

  assign rst_gated_ni = rst_ni & ~clear_i;

  always_ff @(posedge clk_i) rst_q_ni <= clear_i;
endmodule
