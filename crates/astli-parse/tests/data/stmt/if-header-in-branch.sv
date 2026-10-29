// A branch that holds only an `if` header, its body after the `endif`,
// with another conditional straight after.
module m;
  initial begin
`ifdef A
    if (a)
`endif
`ifdef B
`endif
    y = 2;
  end
endmodule
