// A non-blocking assignment in `always_comb` is flagged; a blocking one, or a
// non-blocking one in `always_ff`, is not.
module comb (
  input  logic clk_i,
  input  logic a_i,
  output logic b_o,
  output logic c_o
);
  always_comb begin
    b_o = a_i;
    c_o <= a_i;
    if (a_i) begin
      c_o <= 1'b0;
    end
  end

  always_ff @(posedge clk_i) c_o <= a_i;
endmodule
