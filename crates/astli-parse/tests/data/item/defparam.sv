// `defparam` overrides a parameter of an instance by its path, with a value
// that may be `min:typ:max`.
module top;
  sub u ();
  defparam u.W = 8, u.inner.D = 1:2:3;
endmodule
