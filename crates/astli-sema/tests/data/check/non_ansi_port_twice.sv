// Not reported: a non-ANSI port list may name one port twice, and a single
// declaration below declares it.
module m (a, a);
  input a;
endmodule
