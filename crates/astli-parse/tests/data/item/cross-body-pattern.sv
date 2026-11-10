// A function in a cross's body may hold an assignment pattern, whose `'{`
// and `}` must not close the body before the bins after it.
module m;
  int c, d;
  covergroup cg;
    coverpoint c;
    coverpoint d;
    cXd: cross c, d {
      function int f(int n);
        g('{n, n});
      endfunction
      bins one = f(1);
    }
  endgroup
endmodule
