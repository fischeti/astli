// An include not found directly before a definition stands at the top of
// the file, not in the definition, so a name of `$unit` is unknown. One at
// the start of a body still stands in that body.
`include "generated.svh"
module m;
  logic x;
  assign x = $unit::ALSO;
endmodule

package p;
  `include "first.svh"
  localparam int A = 1;
endpackage

module n;
  logic y;
  assign y = p::FIRST;
endmodule
