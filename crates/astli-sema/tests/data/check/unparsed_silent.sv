// What sema cannot see, it says nothing about: a port list it could not
// lower, a module a region it could not parse may declare, a name up the
// instance tree. That region stands at the top of the file, where it may
// declare any name, so even `nope` is not undeclared.
module open_ports (.a(x), input logic b);
endmodule

primitive udp (out, in);
  output out;
  input in;
  table 0 : 1; 1 : 0; endtable
endprimitive

module m;
  logic a;
  open_ports u_open (.c(a));
  udp u_udp (a, a);
  assign a = top.u.sig;
  assign a = nope;
endmodule
