// A parameter port list: a declaration writing neither keyword nor type
// continues the one before it. In the body of a module with such a list,
// `parameter` is local; without one it is not, and in a package it never
// is.
module m #(parameter int W = 8, D = 2, localparam type T = logic [W-1:0], int unsigned N = W) ();
  parameter int P = 1;
  localparam int Q [2] = '{1, 2};
endmodule

module n;
  parameter P = 1;
  if (1) begin : g
    parameter Q = P;
  end
endmodule

package p;
  parameter int P = 3;
endpackage
