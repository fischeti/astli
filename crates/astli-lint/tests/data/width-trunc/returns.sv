// What a function returns is sized against its return type, in the body
// that builds the function.
module top #(parameter int W = 4) (
  input logic [7:0] a_i,
  output logic [W-1:0] y_o
);
  function automatic logic [W-1:0] low(input logic [7:0] value);
    return value;
  endfunction

  assign y_o = low(a_i);
endmodule
