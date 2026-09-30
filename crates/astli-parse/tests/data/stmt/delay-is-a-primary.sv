// Outside parentheses a delay is one value: a number, a time, `1step` or a
// name, never an expression. What follows is the statement it holds, even
// where it would continue an expression: `-> ev` triggers an event.
module m;
  initial begin
    #1 -> ev;
    ##1 -> ev;
    #100ns ->> ev;
    #1step x = 1;
    #pkg::DELAY x = 1;
    #top.u_clk[0].period x = 1;
    #`DELAY x = 1;
    #(D / 2) x = 1;
  end
endmodule
