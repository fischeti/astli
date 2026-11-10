// In `#3 'd7`, the delay is `3` and the value `'d7`: a space keeps a size
// from joining an unsized based literal after a delay.
module m;
  int i;
  initial i = #3 'd7;
endmodule
