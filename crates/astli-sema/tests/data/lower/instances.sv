// Instances share their instantiation's parameters; connections are
// positional, named, left open, implicit or wildcard.
module m;
  sub #(8, .T(logic [3:0])) u_a (a, , b), u_b [3:0] (.a(), .b, .c(x[0]), .*);
  sub u_c ();
endmodule

// `defparam` and `bind` name what they reach by a path.
module paths;
  defparam u_a.P = 1, u_b.u_c.Q = 2;
  bind target : u_a, u_b.u_c chk u_one ();
  bind u_a.u_d chk u_two ();
endmodule
