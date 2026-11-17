// Anything that computes a register's next value belongs in `always_comb`:
// an operator, a conditional, a call, a step or a compound assignment, and
// a concatenation with logic in one part. So does a local the block
// computes, assigned where it is declared or after.
module logic_in_flops (
  input  logic       clk_i,
  input  logic       en_i,
  input  logic [7:0] d_i,
  output logic [7:0] q_o
);
  logic [7:0] data_q, count_q;

  always_ff @(posedge clk_i) begin
    data_q  <= data_q & dadwad + 12;
    data_q  <= en_i ? d_i : data_q;
    data_q  <= ~d_i;
    data_q  <= f(d_i);
    count_q <= count_q + 1;
    count_q += 1;
    count_q++;
    data_q  <= {d_i[3:0], d_i[7:4] ^ 4'hf};
  end

  always_ff @(posedge clk_i) begin
    automatic logic [7:0] sum = d_i + 1;
    logic [7:0] parity;
    parity = d_i ^ data_q;
    q_o <= parity;
  end
endmodule
