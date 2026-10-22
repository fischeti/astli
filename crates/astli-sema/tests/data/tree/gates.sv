// A gate or a switch is a leaf of the tree, by its name; a UDP's instance
// has the UDP's body. An instance left unnamed has no place in it.
primitive inv (output y, input a);
  table 0 : 1; 1 : 0; endtable
endprimitive

module top (input logic a, b, output wire y, z);
  and #(1, 2) g_and (y, a, b);
  not (z, a);
  inv u_inv (y, a);
  inv (z, b);
endmodule
