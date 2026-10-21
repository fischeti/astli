// A gate's or a switch's terminals are in order, outputs first, and its
// instances may be unnamed or arrays. A gate may take a strength and a delay
// of up to three values; a pull gate takes one strength.
module gates (input a, b, en, output y, z, inout w, v);
  and #(2, 3) g1 (y, a, b), g2 (z, a, b, en);
  nand (y, a, b);
  not n[1:0] (y, z, a);
  bufif1 (strong0, weak1) #(1:2:3, 4, 5) (y, a, en);
  pullup (strong1) (y);
  pulldown (w);
  tranif0 #1 (w, v, en);
  cmos (y, a, en, b);
  rtran (w, v);
endmodule

// A UDP is instantiated as a module is, with a strength and a delay rather
// than parameters, and unnamed where items stand.
module cells (input s, a, b, output y);
  mux2 u0 (y, s, a, b);
  mux2 (pull0, pull1) #5 u1 (y, s, a, b);
  mux2 #(1, 2) (y, s, a, b);
endmodule
