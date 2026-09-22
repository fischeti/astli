initial begin
  // An `else if` nests to the right.
  if (a) x = 1; else if (b) x = 2; else x = 3;
  // A qualifier belongs to the conditional it qualifies.
  unique if (a) x = 1;
  unique0 if (a) x = 1;
  priority if (a) x = 1;
end
