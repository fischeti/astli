// A register written with `=` races with whatever reads it; with `<=` it
// does not. So does a compound assignment or an increment.
module flops (
  input  logic       clk_i,
  input  logic [7:0] d_i,
  output logic [7:0] q_o
);
  logic [7:0] count_q;

  always_ff @(posedge clk_i) begin
    q_o <= d_i;
    q_o = d_i;
    count_q += 1;
    count_q++;
    --count_q;
  end

  always_ff @(posedge clk_i) q_o = d_i;
endmodule
