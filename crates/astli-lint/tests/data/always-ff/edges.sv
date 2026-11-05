// An `always` waiting only for edges is sequential by intent, with a reset
// or not, and a delay in a non-blocking assignment does not change that. A
// level in its list, `always @*`, or a block that waits again inside, which
// `always_ff` does not allow, are not this rule's.
module edges (
  input  logic clk_i,
  input  logic rst_ni,
  input  logic en_i,
  input  logic d_i,
  output logic q_o
);
  always @(posedge clk_i) q_o <= d_i;
  always @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) q_o <= 1'b0;
    else q_o <= d_i;
  end
  always @(posedge clk_i, negedge rst_ni) q_o <= d_i;
  always @(edge clk_i iff en_i) q_o <= d_i;
  always @(posedge clk_i) q_o <= #1 d_i;
  always @(posedge clk_i or d_i) q_o <= d_i;
  always @(d_i) q_o = d_i;
  always @* q_o = d_i;
  always @(posedge clk_i) begin
    #1 q_o = d_i;
  end
  always @(posedge clk_i) q_o = #1 d_i;
  always @(posedge clk_i) begin
    @(negedge clk_i);
    q_o <= d_i;
  end
  always @(posedge clk_i) wait (en_i) q_o <= d_i;
  always_ff @(posedge clk_i) q_o <= d_i;
  always #5 q_o = ~q_o;
endmodule
