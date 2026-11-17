// What is not a one-bit literal 0 or 1 compared by `==` or `!=` passes: a
// case equality, which answers where the signal alone is `x`; a fill or a
// wider literal; an `x` or a `z`; an unsized number; any other operator.
module others (
  input  logic [1:0] a_i,
  input  logic       b_i,
  output logic       y_o
);
  assign y_o = (b_i === 1'b1) || (b_i !== 1'b0);
  assign y_o = (a_i == '1) || (a_i == 2'b01) || (a_i != '0);
  assign y_o = (b_i == 1'bx) || (b_i != 1'bz);
  assign y_o = (b_i == 1) || (b_i != 0);
  assign y_o = (b_i & 1'b1) | (b_i ^ 1'b0);
  assign y_o = 1'b1 == 1'b1;
endmodule
