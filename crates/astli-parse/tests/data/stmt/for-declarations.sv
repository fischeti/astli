initial begin
  // Each declaration in a `for` header has a type of its own, and `var` may
  // come before it: a name after a `,` continues the declaration only when
  // its value follows.
  for (int i = 0, j = 1; i < 4; i++, j++) x = i;
  for (int i = 0, state_e s = s.first(); i < s.num(); i += 1, s = s.next()) x = i;
  for (var int i = 1, bit c = 1'b0; i < 4; i++) x = c;
end
