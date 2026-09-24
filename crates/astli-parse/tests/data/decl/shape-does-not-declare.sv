module m;
  // A name the `(` follows is an instantiation.
  my_module inst (.a(b));
  initial begin
    // One name is an expression.
    unknown_t;
    x [3:0] = y;
    // `int'(x)` begins exactly as `int x` does; the `'` is the difference.
    int'(vs1) % 2 == 0;
  end
endmodule
