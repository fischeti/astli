// A bound instance is held to its definition as any instance is, and its
// target must be declared, even one before `:` whose list names an
// instance. A name the target does not declare is not an error, since it
// may be a net the target declares by using it.
module target;
  logic q;
endmodule

module chk (input logic a);
endmodule

bind target chk u_chk (.a(q), .b(q));
bind target chk u_implicit (.a(maybe_net));
bind nowhere chk u_lost (.a(q));

module top;
  target u_t ();
  bind elsewhere : u_t chk u_listed (.a(q));
endmodule
