// Names from packages: imported by wildcard in the header or the body,
// imported by name, or written `pkg::name`; what a package exports; and a
// package no file declares, whose members are unknown and whose name is
// undeclared.
package base_pkg;
  parameter int Width = 8;
  typedef logic [Width-1:0] word_t;
endpackage

package top_pkg;
  import base_pkg::*;
  export base_pkg::word_t;
  function automatic int twice(int v);
    return 2 * v;
  endfunction
endpackage

module m import top_pkg::*; ();
  import base_pkg::Width;
  word_t w;
  logic [Width-1:0] v;
  assign v = top_pkg::twice(base_pkg::Width) + top_pkg::nope;
  assign w = missing_pkg::thing + top_pkg::word_t'(0);
endmodule

module n;
  import missing_pkg::*;
  assign q = maybe_from_it;
endmodule
