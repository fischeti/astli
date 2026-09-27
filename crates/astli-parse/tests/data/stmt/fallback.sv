// A statement no rule takes goes to the fallback, one statement at a time,
// and the block around it still closes.
initial begin
  wait_order (e1, e2);
  x = 3;
end
