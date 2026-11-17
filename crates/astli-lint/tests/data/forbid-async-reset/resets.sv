// A process that waits for an edge besides its clock's resets
// asynchronously, whichever signal it is, written or from a macro. One that
// waits for the clock alone, or for both of its edges, does not.
`define FF(q, d) always_ff @(posedge clk_i or negedge rst_ni) if (!rst_ni) q <= '0; else q <= (d);

module resets (
  input  logic clk_i,
  input  logic rst_ni,
  input  logic d_i,
  output logic q_o
);
  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) q_o <= 1'b0;
    else q_o <= d_i;
  end
  `FF(q_o, d_i)

  always_ff @(posedge clk_i) begin
    if (!rst_ni) q_o <= 1'b0;
    else q_o <= d_i;
  end
  always_ff @(posedge clk_i or negedge clk_i) q_o <= d_i;
endmodule
