// A conditional among the entries of a list holds entries, and the commas
// after them; one among the arms of a `case` holds arms. A type parameter's
// override is a type, and a connection may carry attributes.
module m (
  input a,
`ifdef X
  input x,
`endif
  output y
);
  foo #(
`ifdef QUESTA
    .T (logic [W-1:0]),
`else
    .T (t_t),
`endif
    .D (2)
  ) i_foo ((* async *) .a (b), .c ());
  always_comb begin
    unique case (s) inside
      [0:3]: y = 1;
`ifdef X
      pkg::A: y = 2;
`endif
      default: y = 0;
    endcase
  end
  if (W < 1) $error("W");
  `ITEM;
endmodule
