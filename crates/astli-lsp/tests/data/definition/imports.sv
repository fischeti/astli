// An import's package and item go to theirs, and a `bind`'s target to the
// definition it binds into.
package imports_pkg;
  typedef logic [3:0] nibble_t;
endpackage

module imports;
  import imports_pkg::nibble_t;
//       ^             ^
  nibble_t n;
//^
endmodule

module probe_m;
endmodule

bind imports probe_m probe ();
//   ^       ^
