// A bound instance's names are the target's, not those where the `bind`
// stands: `q` is the target's, `n` is no name there, and `p` of `top` is
// not seen. Into a target no file declares, every name is unknown.
module target;
  logic q;
endmodule

module chk (input logic a, b, c);
endmodule

module top;
  logic p;
  bind target chk u_chk (.a(q), .b(n), .c(p));
endmodule

bind nowhere chk u_lost (.a(q), .b(), .c());
