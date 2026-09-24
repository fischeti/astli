// Header imports stay in the header, each on a line of its own one level in,
// and the parameter or port list starts the line after.
module m import pkg::*; #(parameter int W = 8) (
  input logic clk
);
endmodule
module n
    import a_pkg::*;
    import b_pkg::*;
(
  input logic clk
);
endmodule
module o import c_pkg::*; ();
endmodule
