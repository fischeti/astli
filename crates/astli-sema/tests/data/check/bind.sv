// A bound instance is held to its definition as any instance is, and its
// target must be declared. A name the target does not declare is not an
// error, since it may be a net the target declares by using it.
module target;
  logic q;
endmodule

module chk (input logic a);
endmodule

bind target chk u_chk (.a(q), .b(q));
bind target chk u_implicit (.a(maybe_net));
bind nowhere chk u_lost (.a(q));
