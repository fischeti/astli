// A one-bit literal 0 or 1 compared by `==` or `!=` is the signal or its
// `!`: on either side, in any base, signed or not, wherever it stands. A
// case equality, a fill, a wider or unsized literal, an `x` and a
// constraint, which states the value the solver is to pick, pass.
module comparisons (
  input  logic [1:0] a_i,
  input  logic       b_i,
  output logic       y_o
);
  assign y_o = (b_i == 1'b1) || (1'b0 != b_i);
  assign y_o = (b_i == 1'h1) || (b_i == 1'sb0);

  assign y_o = (b_i === 1'b1) || (a_i == '1) || (a_i == 2'b01);
  assign y_o = (b_i == 1'bx) || (b_i == 1) || (b_i & 1'b1);
endmodule

class constraints;
  rand bit a;
  constraint a_c {a == 1'b0;}
endclass
