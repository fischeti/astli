// A parameter's type may depend on one before it, and its values follow
// what the top sets that one to.
module tb;
  parameter int W = 4;
  parameter logic [W-1:0] Mask = 0;
  parameter type T = logic [W-1:0];
endmodule

//> values tb Mask
//> values tb Mask W=1
//> set tb Mask=15
//> set tb Mask=16
//> set tb W=8 Mask=255
//> set tb T=1
