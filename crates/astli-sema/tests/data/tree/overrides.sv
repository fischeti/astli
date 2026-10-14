// An instance's overrides, positional or named, values or types, are
// evaluated where it stands; a parameter derived from one follows it. An
// instance array has one body, and a definition no file declares is said so.
module leaf #(parameter int W = 1, parameter type T = logic) ();
  localparam int Twice = W * 2;
endmodule

module mid #(parameter int N = 4, parameter int D = N + 1) ();
  leaf #(.W(D)) u_leaf ();
  leaf u_plain ();
endmodule

module top;
  localparam int Base = 3;
  mid #(Base * 2) u_mid ();
  mid #(.N(4), .D(5)) u_default ();
  leaf #(.T(logic [7:0])) u_typed ();
  leaf #(.W(Base)) u_array [Base-1:0] ();
  absent u_absent ();
endmodule
