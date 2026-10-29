// What is not lowered keeps the names it spells: the constraints of an
// object's `randomize`, which name its members first.
module m;
  initial ok = c.randomize() with { x < 4; };
endmodule
