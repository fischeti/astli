// An argument is sized against the input of the subroutine it is passed
// to; a constant needs only the bits of its value.
package p;
  function automatic int unsigned twice(input int unsigned value);
    return 2 * value;
  endfunction
endpackage

module top (
  input logic [3:0] a_i,
  output int unsigned y_o,
  output int unsigned z_o
);
  assign y_o = p::twice(a_i);
  assign z_o = p::twice(3);
endmodule
