// A flop takes one reset: the one it waits for, tested first. One that
// tests another reset first is told so, and a second reset, tested in its
// `else`, is a read.
module one_reset (
  input  logic clk_i,
  input  logic rst_ni,
  input  logic rst_aon_ni,
  input  logic d_i,
  output logic q_o
);
  always_ff @(posedge clk_i or negedge rst_aon_ni) begin
    if (!rst_ni) q_o <= 1'b0;
    else q_o <= d_i;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) q_o <= 1'b0;
    else if (!rst_aon_ni) q_o <= 1'b0;
    else q_o <= d_i;
  end
endmodule
