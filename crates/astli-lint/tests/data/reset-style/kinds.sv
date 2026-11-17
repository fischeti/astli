// By default a flop resets asynchronously: a synchronous reset is reported.
// An asynchronous one is tested at the level its edge leads to, and first,
// however the test is written.
module kinds (
  input  logic       clk_i,
  input  logic       rst_ni,
  input  logic [7:0] d_i,
  output logic [7:0] q_o
);
  always_ff @(posedge clk_i) begin
    if (!rst_ni) q_o <= '0;
    else q_o <= d_i;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (rst_ni) q_o <= d_i;
    else q_o <= '0;
  end

  always_ff @(posedge clk_i or posedge rst_ni) begin
    if (!rst_ni) q_o <= '0;
    else q_o <= d_i;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    q_o <= d_i;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (rst_ni == 1'b1) q_o <= d_i;
    else q_o <= '0;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (rst_ni != 1'b1) q_o <= '0;
    else q_o <= d_i;
  end
endmodule
