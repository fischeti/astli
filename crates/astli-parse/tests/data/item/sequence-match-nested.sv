// A sequence that assigns as it matches may stand in a second pair of
// parentheses, with nothing else inside the outer one. A call's arguments
// further in leave the outer pair an expression.
property p;
  realtime t;
  @(posedge clk) ((1, t = $realtime)) |=> ($realtime - t == 1);
endproperty

property q;
  @(posedge clk) (f(a, b)) |-> c;
endproperty
