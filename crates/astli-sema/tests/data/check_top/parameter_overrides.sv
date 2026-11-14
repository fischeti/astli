// An override replaces a parameter's default, so the default is checked
// only where it is used: one that fits its declaration's other defaults is
// fine in an instance that overrides it, and wrong in one that overrides
// only what it depends on. An override that does not fit is still wrong.
//
// A parameter with neither type nor range takes its value's type in each
// instance apart, so two instances given members of two enums, of one
// value, each check against their own.
typedef enum logic [1:0] {A, B, C} first_e;
typedef enum logic [1:0] {X, Y} second_e;

module child #(
  parameter int N = 3,
  parameter int VALS [N] = '{1, 2, 3}
) (output logic o);
  assign o = VALS[0] == 4;
endmodule

module holder #(parameter RESET = 0) (output logic [1:0] q);
  assign q = RESET;
endmodule

module top (output logic o, output logic [1:0] q1, output logic [1:0] q2);
  child #(.N(2), .VALS('{4, 5})) u_both (.o());
  child #(.N(2)) u_size (.o());
  child #(.N(2), .VALS('{4, 5, 6})) u_wrong (.o());
  holder #(.RESET(B)) u_first (.q(q1));
  holder #(.RESET(Y)) u_second (.q(q2));
endmodule
