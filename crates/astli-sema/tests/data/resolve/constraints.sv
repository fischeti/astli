// A constraint's names are found where the `randomize` stands, a
// `foreach`'s variable in its loop; an object's `randomize` names its
// members first, so what it constrains is unknown. A `let` finds its
// formals, and a call of it names it. A deferred assertion's label names
// it.
module m;
  int a, b, q[4];
  let twice(x) = x * 2;
  initial begin
    void'(std::randomize(a) with { a < twice(b); foreach (q[i]) q[i] < i; nope > 0; });
    void'(obj.randomize() with { size < a; });
  end
  a_pos: assert final (a >= 0);
  initial $assertoff(0, a_pos);
endmodule
