// A class on the way to a type may take parameters: a `::` after the `)`
// says they are the class's, not the type's.
package p;
  class types #(parameter int W = 8);
    typedef logic [W-1:0] word_t;
    localparam int CONST = W;
  endclass
  class outer #(parameter int A = 1);
    class inner #(parameter int B = 2);
      typedef logic [A+B-1:0] word_t;
    endclass
  endclass
  typedef types#(4)::word_t pkg_word_t;
endpackage
module m #(parameter int W = 8, parameter type T = p::types#(W)::word_t) (input p::types#(W)::word_t i_a);
  import p::*;
  typedef types#(W)::word_t word_t;
  types#(.W(W))::word_t c;
  outer#(1)::inner#(2)::word_t d;
  // Without the `::`, the parameters are the type's.
  mailbox #(int) mb;
  // In an expression, the same scope names a value.
  localparam int K = types#(4)::CONST;
endmodule
