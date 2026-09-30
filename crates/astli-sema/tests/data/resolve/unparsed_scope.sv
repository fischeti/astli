// A scope holding what the parser kept as written may declare anything, so
// a name found nowhere in it is unknown; one found is still found.
module m;
  logic a;
  primitive_like #;
  assign a = b;
endmodule
