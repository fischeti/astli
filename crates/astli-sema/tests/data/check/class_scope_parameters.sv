// A class scope with parameters names a member of the class, in a type as
// in an expression.
package p;
  class types #(parameter int W = 8);
    typedef logic [W-1:0] word_t;
    localparam int CONST = W;
  endclass
  typedef types#(4)::word_t pkg_word_t;
endpackage
module m #(parameter int W = 8) (input p::types#(W)::word_t i_a);
  import p::*;
  typedef types#(W)::word_t word_t;
  types#(.W(W))::word_t c;
  types#(W)::missing_t d;
  localparam int K = types#(4)::CONST;
endmodule
