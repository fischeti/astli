// ANSI ports: a name alone continues the port before it, direction and
// type; a type alone keeps the direction; the first port without one is an
// inout. An interface port, named or generic, has no direction.
module m (
  logic [3:0] first,
  input logic [7:0] a, b,
  output c,
  var logic [1:0] d [2],
  axi_if.mst bus,
  interface.slv generic,
  output logic q = 1'b0
);
endmodule
