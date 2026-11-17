// A reset read anywhere but as a flop's first branch, alone, is logic: in
// combinational logic, in a flop's data, folded into a condition beside a
// clear, beside another statement, or gated on its way to a port.
module reads (
  input  logic       clk_i,
  input  logic       rst_ni,
  input  logic       flush_i,
  input  logic       ready_i,
  input  logic [7:0] d_i,
  output logic       ready_o,
  output logic [7:0] q_o
);
  logic last_q;

  assign ready_o = ready_i && rst_ni;

  always_comb begin
    if (!rst_ni) q_o = '0;
    else q_o = d_i;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni || flush_i) q_o <= '0;
    else q_o <= d_i;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) q_o <= '0;
    else q_o <= d_i;
    last_q <= rst_ni;
  end

  sub u_sub (.rst_ni(rst_ni & ready_i));
endmodule
