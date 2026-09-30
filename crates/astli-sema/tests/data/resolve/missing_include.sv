// An include not found may have declared anything: a name found nowhere in
// the package holding it is unknown there, and to all that imports it; a
// scope that does not import it still finds a name undeclared.
package p;
  `include "generated.svh"
  localparam int A = 1;
endpackage

module m;
  import p::*;
  logic x;
  assign x = A + FROM_HEADER + p::ALSO;
endmodule

module n;
  logic y;
  assign y = nowhere;
endmodule
