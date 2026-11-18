// Parentheses, a chain of one operator, operators of one level, and an
// operand in a call's or an index's brackets pass; so do arithmetic, a
// comparison inside `&&`, and a bitwise operator inside `&&` or `||`.
module clear (
  input  logic [3:0] a_i,
  input  logic       b_i,
  input  logic       c_i,
  output logic       y_o
);
  assign y_o = b_i || (c_i && b_i);
  assign y_o = b_i && c_i && b_i;
  assign y_o = b_i ^ c_i ~^ b_i;
  assign y_o = b_i || $isunknown(b_i && c_i) || a_i[b_i && c_i];
  assign y_o = a_i + a_i * a_i == a_i;
  assign y_o = a_i == 4'h1 && (b_i || c_i & b_i);
endmodule
