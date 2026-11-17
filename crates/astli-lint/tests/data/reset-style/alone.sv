// An asynchronous reset takes all of its process: a flop beside its branch
// waits for the reset's edge without taking it. Its branch may stand in a
// loop over an array, which resets each element.
module alone (
  input  logic       clk_i,
  input  logic       rst_ni,
  input  logic       d_i,
  output logic       q_o,
  output logic       r_o,
  output logic [3:0] a_o
);
  always_ff @(posedge clk_i or negedge rst_ni) begin
    for (int i = 0; i < 4; i++) begin
      if (!rst_ni) a_o[i] <= 1'b0;
      else a_o[i] <= d_i;
    end
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    r_o <= d_i;
    if (!rst_ni) q_o <= 1'b0;
    else q_o <= d_i;
  end
endmodule
