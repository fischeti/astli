// An instance's overrides, positional or named, values or types, are
// evaluated where it stands; a parameter derived from one follows it. An
// instance array has one body, and a definition no file declares is said so.
// A local parameter takes no place in the ordered list, even in a parameter
// port list, so `#(12, 16)` gives `D` the `16`.
module leaf #(parameter int W = 1, parameter type T = logic) ();
  localparam int Twice = W * 2;
endmodule

module mid #(parameter int N = 4, parameter int D = N + 1) ();
  leaf #(.W(D)) u_leaf ();
  leaf u_plain ();
endmodule

module mem #(parameter int A = 16, localparam int M = 1 << A, parameter int D = 8) ();
endmodule

module top;
  localparam int Base = 3;
  mid #(Base * 2) u_mid ();
  mid #(.N(4), .D(5)) u_default ();
  leaf #(.T(logic [7:0])) u_typed ();
  leaf #(.W(Base)) u_array [Base-1:0] ();
  mem #(12, 16) u_mem ();
  absent u_absent ();
endmodule
