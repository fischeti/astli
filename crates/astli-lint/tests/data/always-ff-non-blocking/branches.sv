// Every branch of an `ifdef` is read, and a macro call is not looked into.
module branches (
  input  logic clk_i,
  input  logic d_i,
  output logic q_o
);
  always_ff @(posedge clk_i) begin
`ifdef SIM
    q_o = d_i;
`else
    q_o <= d_i;
`endif
    `SET(q_o, d_i)
  end
endmodule
