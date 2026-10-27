// A constraint reads what it names, and a `let` what it is passed. A
// deferred assertion among items reads its condition as a process does.
module m;
  int a, b, c;
  let twice(x) = x * 2;
  initial void'(std::randomize(a) with { a < twice(b); });
  assert final (c > 0);
endmodule
