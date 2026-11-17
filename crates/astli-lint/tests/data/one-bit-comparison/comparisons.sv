// A one-bit signal compared with `1'b1` is the signal, and with `1'b0` its
// `!`, wherever the comparison stands, the literal on either side, in any
// base, signed or not.
module comparisons (
  input  logic clk_i,
  input  logic a_i,
  input  logic b_i,
  output logic y_o,
  output logic z_o
);
  assign y_o = (a_i == 1'b1) && (b_i != 1'b0);
  assign z_o = 1'b0 == a_i ? b_i : a_i;

  always_comb begin
    if (a_i == 1'h1) z_o = b_i != 1'd1;
    else z_o = a_i == 1'sb0;
  end

  always_ff @(posedge clk_i) begin
    if (a_i == 1'b1) y_o <= b_i;
  end
endmodule
