// A name declared nowhere, a package no file declares, a name a package
// lacks, imported or written `pkg::name`; each reported once, where it is
// written. A wildcard import of a package no file declares may supply any
// name, so nothing in its scope is undeclared.
package p;
  localparam int A = 1;
endpackage

module m;
  import p::A;
  import p::B;
  logic x;
  assign x = y + p::C + r::D + A;
endmodule

module n;
  import q::*;
  logic x;
  assign x = z;
endmodule
