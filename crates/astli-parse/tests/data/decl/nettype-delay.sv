// A net of a user-defined nettype may take a delay, as a net of a built-in
// type does.
module m;
  nettype logic [1:0] nt;
  nettype real rt;
  nt #3 b;
  rt #3.4 h = 1;
endmodule
