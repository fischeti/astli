// A parameter's value written as a named type with packed dimensions keeps
// the space before them, as a declaration's type does: it parses as a
// part-select of the name, and only the name's declaration could tell. One
// written without it, and a select that is not a range, stay tight.
module m;
  pass #(.data_t(nib_t   [N-1:0]), .mask_t(p::nib_t [1:0] [3:0])) u_named ();
  pass #(nib_t [15:0], Masks [7:0][3:0]) u_positional ();
  pass #(.Mask(Masks[3:0]), .W(Widths [3]), .V(Values [B +: 4])) u_selects ();
  c #(nib_t [3:0])::word_t word;
  assign y = x [3:0];
endmodule
