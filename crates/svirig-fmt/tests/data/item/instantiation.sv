// Named parameters and connections go one per line; positional ones stay on
// the line if they fit.
module m;
  foo #(.Width(8), .Depth(4)) u_foo (.clk_i, .d_i (d), .q_o( q ));
  bar #(16) u_bar (x, y);
  baz u_baz ();
  qux u_qux (.*);
  quux u_a (.a(a)), u_b (.a(b));
  a_module_with_a_long_name u_a_module_with_a_long_name (first_signal, second_signal, third);
endmodule
