// A non-ANSI port may be a concatenation of names, and an interface port is
// declared below the header by its interface's name, with or without a
// modport.
module n ({a, b}, c, x, y);
  input a;
  output b;
  input c;
  bus x;
  bus.mp y;
endmodule
