// `defparam` lines up its `=` with the others of a run, as `assign` does.
module top;
  defparam u.W=8;
  defparam u.inner.DEPTH = 1:2:3, u.N = 2;
endmodule
