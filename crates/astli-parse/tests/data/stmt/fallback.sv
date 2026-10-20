// A statement no rule takes goes to the fallback, one statement at a time,
// and the block around it still closes. A gate is no statement.
initial begin
  and g1 (y, a, b);
  x = 3;
end
