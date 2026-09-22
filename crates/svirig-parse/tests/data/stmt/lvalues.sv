initial begin
  a[3:0] = b;
  {a, b} = c;
  x.y.z = 1;
  mem[i][j] = 0;
  // An assignment may be delayed by its own operator.
  a <= #1 b;
end
