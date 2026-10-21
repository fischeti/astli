// A gate's output drives a variable as a continuous assignment does, so a
// second driver clashes with it, an unnamed one too. A name its delay reads
// is resolved, and one declared nowhere is an error; one it connects
// declares a net.
primitive inv (output y, input a);
  table 0 : 1; 1 : 0; endtable
endprimitive

module top (input logic a, b);
  logic y, z;
  and g_and (y, a, b);
  assign y = a;
  inv (z, a);
  assign z = b;
  or #(NOPE) (w, a, implicit);
endmodule
