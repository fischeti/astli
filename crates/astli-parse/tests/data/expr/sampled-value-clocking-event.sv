// A sampled value function takes a clocking event as its last argument.
module m (input logic clk, rst_n, a);
  assert property (@(posedge clk) disable iff ($fell(rst_n, @(posedge clk))) a);
  assert property (@(posedge clk) $rose(a, @(posedge clk)));
  assert property (@(posedge clk) $past(a, 1, 1'b1, @(posedge clk)) |-> a);
  assert property (@(posedge clk) $stable(a, @clk));
endmodule
