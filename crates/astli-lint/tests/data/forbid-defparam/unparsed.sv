// `defparam` is found even where the parser keeps it as written.
module top;
  sub u_sub ();
  defparam u_sub.Width = 16;
  sub #(.Width(16)) u_other ();
endmodule
