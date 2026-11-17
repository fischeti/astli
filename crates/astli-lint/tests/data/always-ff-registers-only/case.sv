// A `case` picks among values, which is a mux: select in `always_comb`, and
// register what it picked. Every kind of `case` is one.
module case_in_flops (
  input  logic       clk_i,
  input  logic [1:0] sel_i,
  input  logic [7:0] a_i,
  input  logic [7:0] b_i,
  output logic [7:0] q_o
);
  always_ff @(posedge clk_i) begin
    case (sel_i)
      2'd0:    q_o <= a_i;
      default: q_o <= b_i;
    endcase
    unique casez (sel_i)
      2'b1?:   q_o <= a_i;
      default: q_o <= b_i;
    endcase
  end
endmodule
