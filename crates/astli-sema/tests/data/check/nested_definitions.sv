// Not reported: a module or checker declared inside another is a definition
// its parent can instantiate, and a checker a package declares is one an
// import or a package-scoped name makes visible, to an instance or a `bind`.
package p;
  checker pc(a, b);
  endchecker
endpackage

module m (input wire clk);
  module n;
    initial @(posedge clk);
  endmodule

  n u_n ();
endmodule

checker outer(q);
  checker inner(r);
    int k = q + r;
  endchecker

  inner u_inner (q);
endchecker

module uses;
  import p::*;
  wire e, f;
  pc u_imported (e, f);
  p::pc u_scoped (e, f);
  outer u_outer (e);
endmodule

module binds;
  uses u_uses ();
  bind uses pc u_bound (e, f);
  bind uses p::pc u_bound_scoped (e, f);
endmodule
