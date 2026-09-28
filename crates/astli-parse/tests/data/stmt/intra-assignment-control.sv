// An assignment may wait before it takes its value: for a delay, an event,
// or an event a number of times. So may a nonblocking trigger.
module m;
  initial begin
    a = #5 b;
    a <= @(posedge clk) b;
    a = repeat (3) @(posedge clk) b;
    a <= repeat (n) @(negedge clk or posedge rst) b;
    ->> repeat (2) @(posedge clk) done;
  end
endmodule
