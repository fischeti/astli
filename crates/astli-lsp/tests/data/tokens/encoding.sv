// Before a name on its line, `𝄞` is two UTF-16 code units and `é` one.
module encoding;
  logic x;
  /* 𝄞é */ assign x = 1'b0;
endmodule
