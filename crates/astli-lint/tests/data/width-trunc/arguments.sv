// An argument is sized against the input of the subroutine it is passed
// to, as the body calling it builds that input's type.
module top #(parameter int W = 4) (
  input logic [7:0] a_i,
  output logic [3:0] y_o
);
  function automatic logic [W-1:0] low(input logic [W-1:0] value);
    return value;
  endfunction

  assign y_o = low(a_i);
endmodule
