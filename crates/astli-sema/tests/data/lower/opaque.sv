// What is not lowered keeps the names it spells: the constraints of a
// `randomize` restricted to the names it lists.
module m;
  initial ok = c.randomize() with (x) { x < 4; };
endmodule
