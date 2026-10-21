// A gate drives its first terminal and reads the rest; a `buf` or a `not`
// drives all but its last; a pass switch carries both ways between its
// first two; a pull gate drives what it is connected to. A UDP's ports say
// how each is used, as a module's do.
primitive inv (output y, input a);
  table 0 : 1; 1 : 0; endtable
endprimitive

module m (input wire a, b, en, output wire y, z, inout wire p, q);
  parameter int D = 2;
  and #(D) (y, a, b);
  buf (y, z, a);
  tranif1 (p, q, en);
  pullup (q);
  inv (z, en);
endmodule
