// A property may be a `case` over properties, and an `if` property may
// stand in parentheses as an operand.
module m;
  logic clk, a, b, c;
  cover property (@(posedge clk) case (b) 1'b0: 1 ##1 b; 1'b1: a and b; default: 1 or b; endcase);
  assert property (@(posedge clk) (if (b) a else c));
  assert property (@(posedge clk) a and (if (b) (a |=> c) else (c)));
endmodule
