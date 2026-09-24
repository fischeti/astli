module m;
  // Neither name is known to this file. The `(` is the whole of what
  // separates an instantiation from a declaration.
  foo_t x;
  foo u_foo (.a(b), .*);
  // Parameter overrides, and several instances in one statement.
  foo #(.W(8)) u_a (), u_b [1:0] ();
endmodule
