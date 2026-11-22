// `reg` is fixed to `logic`. A name nothing declares is found too, and has
// no fix to offer.
module reg_fix (
  input logic clk_i
);
  reg q;
  wire p = nowhere;
  always_ff @(posedge clk_i) q <= p;
endmodule
