// `randomize() with` takes constraints, where an array method's `with`
// takes an expression in parentheses.
module m;
  initial begin
    ok = std::randomize(x) with { x < 4; x > 0; };
    ok = obj.randomize() with (a) { a -> b; };
    n = q.sum() with (int'(item));
  end
endmodule
