// A `bind` naming a definition binds into that one alone, not into one
// spelled alike that a definition declares inside it, which no name outside
// reaches; one naming only such a definition binds into nothing. So does a
// list after `:` the definition names, but a path reaches an instance of
// either.
module chk;
endmodule

module leaf;
endmodule

module wrap;
  leaf u_outer ();
endmodule

module top;
  module leaf;
  endmodule
  module inner;
  endmodule
  leaf u_leaf ();
  inner u_inner ();
  wrap u_wrap ();
endmodule

bind leaf chk u_chk ();
bind inner chk u_lost ();
bind leaf : top.u_leaf, top.u_wrap.u_outer chk u_listed ();
bind top.u_leaf chk u_path ();
