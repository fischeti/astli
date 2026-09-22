assign x = a inside {1, [2:3]};
assign x = a dist {1 := 2, 3 :/ 4};
// A `with` clause hangs off the call it qualifies.
assign x = q.find with (item > 3);
