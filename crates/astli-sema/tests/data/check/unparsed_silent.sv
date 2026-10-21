// What sema cannot see, it says nothing about: a port list it could not
// lower, a module a region it could not parse may declare, a name up the
// instance tree. That region stands at the top of the file, where it may
// declare any name, so even `nope` is not undeclared.
module open_ports (.a(x), input logic b);
endmodule

+ hidden;

module m;
  wire a;
  open_ports u_open (.c(a));
  hidden u_hidden (a, a);
  assign a = top.u.sig;
  assign a = nope;
endmodule
