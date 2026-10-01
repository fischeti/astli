// A signal or input nothing reads is reported; one read anywhere is not,
// nor an output, which is read outside.
module m (input logic clk_i, input logic spare_i, output logic q_o);
  logic d, q, only_written;
  logic [3:0] mem [4];
  assign d = mem[0][0];
  assign only_written = 1'b1;
  always_ff @(posedge clk_i) q <= d;
  assign q_o = q;
endmodule
