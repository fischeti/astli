// `&&` inside `||`; `&`, `^` or `~^` inside `|`; `&` inside `^`; a
// comparison, `inside` among them, inside `&`, `|` or `^`; and `+` or `-`
// inside a shift, on either side, wherever the expression stands. The fix
// writes the parentheses the parser already put there, around what is
// written, past a comment or an attribute ahead of it, and a macro call is an
// operand like any other. Parentheses, a chain of one operator, operators of
// one level, an operand in a call's or an index's brackets, arithmetic, `*`
// inside a shift, a comparison inside `&&`, and a bitwise operator inside
// `&&` or `||` pass.
module mixed (
  input  logic       a_i,
  input  logic       b_i,
  input  logic       c_i,
  input  logic [3:0] x_i,
  input  logic [3:0] n_i,
  output logic       y_o,
  output logic [7:0] z_o,
  output logic       y_q
);
  assign y_o = a_i || b_i && !c_i;
  assign y_o = a_i && b_i || c_i && a_i;
  assign y_o = a_i | b_i & c_i;
  assign y_o = a_i ^ b_i | c_i ~^ a_i;
  assign y_o = a_i ^ b_i & c_i;
  assign y_o = a_i & x_i == 4'h1;
  assign y_o = x_i != 4'h0 | a_i;
  assign y_o = a_i ^ x_i >= 4'h2;
  assign y_o = x_i inside {4'h1, 4'h2} | a_i;
  assign z_o = x_i << n_i + 1;
  assign z_o = x_i - 1 >>> n_i;
  assign y_o = a_i || /* why */ b_i && (* keep *) c_i;
  assign y_o = a_i || `FLAG && b_i;
  always_ff @(posedge a_i) y_q <= b_i || c_i && a_i;

  assign y_o = a_i || (b_i && c_i);
  assign y_o = (x_i & n_i) == 4'h1;
  assign z_o = x_i << (n_i + 1);
  assign y_o = a_i && b_i && c_i;
  assign y_o = a_i ^ b_i ~^ c_i;
  assign y_o = a_i || $isunknown(b_i && c_i) || x_i[b_i && c_i];
  assign y_o = x_i + x_i * x_i == x_i;
  assign z_o = x_i << n_i * 2;
  assign y_o = x_i == 4'h1 && n_i < 4'h2;
  assign y_o = a_i && x_i inside {4'h1};
  assign y_o = a_i & b_i && b_i | c_i;
  assign y_o = a_i || b_i ^ c_i;
endmodule

class constraints;
  rand bit a, b, c;
  constraint a_c {a -> b || c && a;}
endclass
