// A net's `=` is a continuous assignment, a parameter's is a constant, and a
// `genvar`'s is its loop's start. Procedures, subroutines, classes and
// packages set their variables' values where they run, which is another
// question.
module accepted (
  input logic a_i
);
  localparam logic One = 1'b1;
  wire driven = a_i;
  wire logic [1:0] both = {a_i, a_i};
  uwire single = a_i;
  logic declared;

  for (genvar i = 0; i < 2; i++) begin : gen_loop
    wire looped = a_i;
  end

  always_comb begin
    automatic logic tmp = a_i;
    declared = tmp;
  end

  function automatic logic invert(logic value);
    logic result = !value;
    return result;
  endfunction
endmodule

package accepted_pkg;
  logic flag = 1'b0;
endpackage

class accepted_c;
  int count = 0;
endclass
