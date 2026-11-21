// A comparison is sized wherever a body evaluates it: in a `case` label, a
// timing, an argument, a cast, a select's index, a pattern, a replication,
// and a compound assignment's value.
module top (
  input logic c,
  input logic [7:0] a_i
);
  logic [3:0] n, k;
  logic [1:0] p [2];
  int m;

  always_comb begin
    case (1'b1)
      a_i == 4'd1: n = 4'd0;
      default: n = 4'd1;
    endcase
    $display(a_i == 4'd2);
    m = int'(a_i == 4'd3);
    n[a_i == 4'd4] = 1'b1;
    p = '{a_i == 4'd5, 1'b0};
    n = {4{a_i == 4'd6}};
    m += a_i == 4'd7;
  end

  always @(posedge c iff (a_i == 4'd8)) k = 4'd0;
endmodule
