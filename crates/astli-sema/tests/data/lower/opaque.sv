// What is not lowered keeps the names it spells: a class body, the
// constraints of an object's `randomize`, which name its members first.
module m;
  class C extends B;
    int x;
  endclass
  initial ok = c.randomize() with { x < 4; };
endmodule
