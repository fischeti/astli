// The constraints of a `randomize` are a block of them: each one an
// expression, an implication an `if` with no `else`, and an `if` or a
// `foreach` as in a procedural block. A `dist` keeps each value, bound and
// weight. A `let` is an expression over its formals, inputs unless written
// otherwise.
module m;
  int a, b, q[4];
  let max(x, y = 0) = (x > y) ? x : y;
  initial begin
    void'(std::randomize(a, b, q) with {
      a < 10;
      soft b inside {[1:4]};
      a > 2 -> { b == 1; };
      if (a == 3) b == 2; else b dist { 1 := 2, [3:4] :/ 1 };
      foreach (q[i]) q[i] < max(a, i);
      unique { a, b };
    });
  end
endmodule
