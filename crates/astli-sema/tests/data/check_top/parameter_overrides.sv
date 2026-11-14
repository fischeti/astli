// An override replaces a parameter's default, so the default is checked
// only where it is used: one that fits its declaration's other defaults is
// fine in an instance that overrides it, and wrong in one that overrides
// only what it depends on. An override that does not fit is still wrong.

module child #(
  parameter int N = 3,
  parameter int VALS [N] = '{1, 2, 3}
) (output logic o);
  assign o = VALS[0] == 4;
endmodule

module top (output logic o);
  child #(.N(2), .VALS('{4, 5})) u_both (.o());
  child #(.N(2)) u_size (.o());
  child #(.N(2), .VALS('{4, 5, 6})) u_wrong (.o());
endmodule
