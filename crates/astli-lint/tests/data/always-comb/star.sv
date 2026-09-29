// `always @*` and `always @(*)` are combinational by intent; a sensitivity
// list, an edge, a delay or `always_comb` itself are not this rule's.
module star (
  input  logic clk_i,
  input  logic a_i,
  output logic b_o
);
  always @* b_o = a_i;
  always @(*) b_o = a_i;
  always @( * ) begin
    b_o = a_i;
  end
  always @(a_i) b_o = a_i;
  always @(posedge clk_i) b_o <= a_i;
  always_comb b_o = a_i;
  always #5 b_o = ~b_o;
endmodule
