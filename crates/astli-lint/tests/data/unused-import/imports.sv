// An import nothing in the file uses; a wildcard one used by any name, an
// explicit one by its name; `pkg::name` needs none.
package p;
  localparam int A = 1;
  localparam int B = 2;
  typedef logic [3:0] t;
endpackage

package q;
  localparam int C = 3;
endpackage

module m import p::*; ();
  import p::B;
  import q::*;
  t x;
  assign x = p::A;
endmodule
