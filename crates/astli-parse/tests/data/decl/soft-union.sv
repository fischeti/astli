// SystemVerilog 2023: a packed union may be `soft`, its members of
// different widths.
module m;
  union soft {
    logic [7:0] a;
    logic [5:0] b;
  } u;
endmodule
