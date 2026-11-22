// An import names a package, and an item the package declares; an item it
// lacks is undeclared, and a package no file declares is none.
package pkg;
  typedef logic [7:0] byte_t;
endpackage

module top;
  import pkg::byte_t;
//       ^    ^
  import pkg::missing;
//            ^
  import pkg::*;
//       ^
  import elsewhere::*;
//       ^
  byte_t value;
//^
endmodule
