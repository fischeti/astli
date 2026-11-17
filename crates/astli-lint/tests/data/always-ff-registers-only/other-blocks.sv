// Only `always_ff` is judged: logic in `always_comb` is where it belongs,
// and an `always` or `initial` is another rule's business.
module other_blocks (
  input  logic       clk_i,
  input  logic [1:0] sel_i,
  input  logic [7:0] a_i,
  input  logic [7:0] b_i,
  output logic [7:0] q_o
);
  logic [7:0] sum, c;

  always_comb begin
    case (sel_i)
      2'd0:    q_o = a_i;
      default: q_o = b_i;
    endcase
    if (sel_i == 2'd3) sum = a_i + b_i;
    else sum = '0;
  end

  always @* c = a_i & b_i;
  always @(posedge clk_i) c <= a_i + 1;
  initial c = a_i | b_i;
endmodule
