// Branching directives sit at the margin, nested or not, and a branch's items
// are indented as if the directives were absent.
module m;
  `ifdef A
    logic a;
  `elsif B // why B
    `ifndef C
      logic c;
    `endif
  `else
  logic d;
  // before the endif
  `endif
endmodule
