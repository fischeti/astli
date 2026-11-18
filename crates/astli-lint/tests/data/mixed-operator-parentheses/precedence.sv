// A comparison inside `&`, `|` or `^` compares before it masks, as RTL's
// one-bit and means it to, and `+` or `-` inside a shift adds before it
// shifts, on either side; parentheses, a comparison inside `&&`, and `*`
// inside a shift pass.
module precedence (
  input  logic [3:0] a_i,
  input  logic [3:0] b_i,
  input  logic       en_i,
  output logic       y_o,
  output logic [7:0] z_o
);
  assign y_o = en_i & a_i == 4'h1;
  assign y_o = a_i != 4'h0 | en_i;
  assign y_o = en_i ^ b_i >= 4'h2;
  assign z_o = a_i << b_i + 1;
  assign z_o = a_i - 1 >>> b_i;

  assign y_o = (a_i & b_i) == 4'h1;
  assign y_o = a_i == 4'h1 && b_i < 4'h2;
  assign z_o = a_i << (b_i + 1);
  assign z_o = a_i << b_i * 2;
endmodule
