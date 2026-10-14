// Names reach constants elsewhere: a package's by import or `pkg::`, the
// file's top level, a parameter of the definition at its default. A
// parameter defined by itself is unknown, as is one with no default.
localparam int Top = 3;

package p;
  localparam int W = 8;
  localparam int D = W * 2;
endpackage

package q;
  import p::*;
  localparam int E = D + 1;
endpackage

module m #(parameter int N = p::W, parameter int K) ();
  import q::E;
  localparam int A = N + E;
  localparam int B = Top;
  localparam int C = C + 1;
  localparam int F = K;
  if (1) begin : g
    localparam int G = A;
  end
endmodule
