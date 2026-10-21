// A gate or a switch is an instance of its keyword, with its delay; one
// left unnamed, a UDP's too, is declared with no name.
module m (input wire a, b, output wire y);
  bufif1 (strong0, weak1) #(1, 2) g_buf (y, a, b), (y, b, a);
  inv #3 (y, a);
endmodule
