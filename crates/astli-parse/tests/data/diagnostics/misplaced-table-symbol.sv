// Which symbol may stand where in a table depends on its text. An edge is
// only an input of a sequential primitive, one per row, between levels; the
// state is a level; the output is `0`, `1` or `x`, or `-` to keep a
// sequential one's.
primitive comb (output y, input a, b);
  table
    r 0 : 1;
    0 1 : b;
    - 1 : 0;
  endtable
endprimitive

primitive seq (output reg q, input clk, d);
  table
    r f : 0 : 1;
    (0r) 1 : ? : 1;
    0 1 : - : ?;
  endtable
endprimitive
