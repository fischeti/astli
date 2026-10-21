// A UDP's ports are declared in its header or after it, the output first. A
// combinational one maps levels to an output; a sequential one maps its
// inputs, one of which may change, and its state to the next state, `-` to
// keep it. Symbols may be written together, `01`, and a directive stands
// among the rows.
primitive mux2 (y, s, a, b);
  output y;
  input s, a, b;
  table
  // s a b : y
     0 1 ? : 1;
     0 0 ? : 0;
     1 ? 1 : 1;
     1?0:0;
`ifdef PESSIMISTIC
     x 1 1 : 1;
`endif
  endtable
endprimitive

primitive dff (output reg q = 1'b0, input clk, d);
  table
    (01) 0 : ? : 0;
    (0?) 1 : ? : 1;
    r    1 : ? : 1;
    n    ? : ? : -;
    ?    * : ? : -;
  endtable
endprimitive : dff

primitive latch (q, en, d);
  output q;
  reg q;
  input en, d;
  initial q = 0;
  table
    1 0 : ? : 0;
    0 ? : ? : -;
  endtable
endprimitive

extern primitive inv (output y, input a);
primitive inv (.*);
  table 0 : 1; 1 : 0; endtable
endprimitive
