// A real has no bits: a select of it, a select by it, and an edge of it are
// refused, while an element of an array of reals and a change of one are
// not.
module m;
  real a = 0.5;
  real arr [4];
  logic [3:0] b;
  wire c, d, e, f;
  assign c = a[2];
  assign d = b[a];
  assign e = b[a+:1];
  assign f = arr[1] > 0.0;
  always @(posedge a) $display("rose");
  initial @(negedge a or edge a) $display("fell");
  always @(a) $display("changed");
endmodule
