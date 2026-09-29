// With more than one port, each is connected by name; `.*` and `.name` are
// by name too. One port may go by position. A macro call among the ports may
// write names, so such an instance is left alone.
module ports;
  sub u_a (a, b);
  sub u_b (.a(a), b);
  sub u_c (.a(a), .b, .*);
  sub u_d (a);
  sub u_e (.a(a), `PORTS);
  sub u_f (), u_g (x, y);
endmodule
