initial begin
  // `<=` both assigns and compares. The left-hand side is an lvalue, not an
  // expression, so the statement rule sees the operator.
  q <= d;
  q <= (a <= b);
end
