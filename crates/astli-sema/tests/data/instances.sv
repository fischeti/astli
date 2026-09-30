// Instances share their instantiation's parameters; connections are
// positional, named, left open, implicit or wildcard.
module m;
  sub #(8, .T(logic [3:0])) u_a (a, , b), u_b [3:0] (.a(), .b, .c(x[0]), .*);
  sub u_c ();
endmodule
