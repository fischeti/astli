// `&&` inside `||`, and `&` or `^` inside `|` or `&` inside `^`, on either
// side, wherever the expression stands: the fix writes the parentheses the
// parser already put there, around what is written, past a comment or an
// attribute ahead of it, and a macro call is an operand like any other.
module mixed (
  input  logic a_i,
  input  logic b_i,
  input  logic c_i,
  input  logic d_i,
  output logic y_o,
  output logic z_q
);
  assign y_o = a_i || b_i && !c_i;
  assign y_o = a_i && b_i || c_i && d_i;
  assign y_o = a_i | b_i & c_i;
  assign y_o = a_i ^ b_i | c_i ~^ d_i;
  assign y_o = a_i ^ b_i & c_i;
  assign y_o = a_i || /* why */ b_i && (* keep *) c_i;
  assign y_o = a_i || `FLAG && b_i;

  always_ff @(posedge a_i) z_q <= b_i || c_i && d_i;
endmodule

class constraints;
  rand bit a, b, c;
  constraint a_c {a -> b || c && a;}
endclass
