// What a branch a constant condition rules out holds is not sized: which
// branch is written for which width is a parameter's to decide.
module top #(parameter bit Wide = 1'b1) (
  input logic [7:0] a_i,
  output logic [7:0] y_o,
  output logic [3:0] z_o
);
  always_comb begin
    if (Wide) y_o = a_i;
    else z_o = a_i;
    if (!Wide) y_o = a_i;
    else z_o = a_i;
  end
endmodule
