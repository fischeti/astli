// A condition is a reset or an enable only if it is one signal, negated or
// not; a gate or a comparison is logic to compute in `always_comb`.
module conditions (
  input  logic       clk_i,
  input  logic       rst_ni,
  input  logic       en_i,
  input  logic       stall_i,
  input  logic [7:0] d_i,
  output logic [7:0] q_o
);
  logic [1:0] cnt_q;

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      q_o <= '0;
    end else if (en_i && !stall_i) begin
      q_o <= d_i;
    end
  end

  always_ff @(posedge clk_i) begin
    if (cnt_q == 2'd3) q_o <= d_i;
    if (!(en_i || stall_i)) q_o <= '0;
    if ((en_i)) q_o <= d_i;
  end
endmodule
