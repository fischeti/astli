// A value truncated to a narrower target or port; and a constant that fits.
// An instance of a UDP may be left unnamed, and is built all the same.
module sub #(parameter int W = 8) (
  input  logic [W-1:0] a_i,
  output logic [W-1:0] y_o
);
  assign y_o = a_i;
endmodule

primitive inv (output o, input a);
  table
    0 : 1;
    1 : 0;
  endtable
endprimitive

module top (
  input  logic [7:0] a_i,
  output logic [3:0] low_o,
  output logic [3:0] port_o,
  output logic inv_o
);
  logic [3:0] count;
  logic [7:0] fits;

  // Truncated.
  assign low_o = a_i;
  // A constant counts the bits its value needs.
  assign fits = 0;
  assign count = 9'd3;
  always_comb begin
    fits = 9'h1ff;
    count = a_i + 1;
  end
  // An output wider than what it drives.
  sub #(.W(16)) u_wide (.a_i(), .y_o(port_o));
  inv (inv_o, a_i);
endmodule
