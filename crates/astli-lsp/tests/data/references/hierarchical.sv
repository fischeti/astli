// A hierarchical name is a use of what it names: a definition named from
// the top, which no instance names, and a member of it.
module hierarchical_top;
//     ^
  logic probe;
//      ^
endmodule

module hierarchical_tb;
  logic seen;
  assign seen = hierarchical_top.probe;
endmodule
