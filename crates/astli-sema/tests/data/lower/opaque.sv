// What is not lowered keeps the names it spells: a class body, the
// constraints of a `randomize`.
module m;
  class C extends B;
    int x;
  endclass
  initial ok = std::randomize(x) with { x < 4; };
endmodule
