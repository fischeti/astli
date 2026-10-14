// A struct's member is no hierarchical name, so `s.valid` does not keep a
// signal `valid` from being reported; a path through an instance reads the
// signal it resolves to, and one through a class handle may read anything
// so named. A prototype's ports have no body to read them.
module leaf;
  logic seen, valid;
  logic maybe;
endmodule

module m;
  typedef struct packed { logic valid; } s_t;
  s_t s;
  leaf u_leaf ();
  class C;
  endclass
  C c;
  import "DPI-C" function void dpi_f(input int value);
  initial $display(s.valid, u_leaf.seen, c.maybe);
endmodule
