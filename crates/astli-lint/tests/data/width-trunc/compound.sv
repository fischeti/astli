// A compound assignment computes at the wider of its target and its value,
// so only a value wider than the target loses bits. A shift's amount is no
// operand of its width.
module top (
  input logic [7:0] a_i,
  input logic [3:0] b_i
);
  logic [3:0] n;
  logic [7:0] m;

  always_comb begin
    n = 4'd0;
    n += a_i;
    n -= b_i;
    n += 300;
    n <<= a_i;
    m = 8'd0;
    m += b_i;
  end
endmodule
