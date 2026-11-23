// restriction: what a macro call writes is reported at the call, and the
// waiver goes on the statement around it, where lint reads it.
`define INVERT(x) ~x

module macro (
  input  logic clk_i,
  input  logic d_i,
  output logic q_o
);
  always_ff @(posedge clk_i) begin
    q_o <= `INVERT(d_i);
  end
endmodule
