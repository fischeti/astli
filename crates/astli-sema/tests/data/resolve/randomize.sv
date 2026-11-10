// An object's `randomize` takes its members as arguments, inherited ones
// included, ahead of names where the call stands. Its constraints after
// `with (x)` find only `x` among them, and `local::` none. `randomize`
// called on no object, `std::randomize` among them, takes the names where
// it stands.
class A;
  rand int x;
  rand int y;
endclass

class B extends A;
  rand int z;
  function void f(int x);
    void'(randomize(x));
    void'(this.randomize(x, z));
  endfunction
endclass

module m;
  A a;
  B b;
  int x, y, z;
  initial begin
    void'(b.randomize(x, y, z));
    void'(a.randomize() with (x) { x < y; });
    void'(a.randomize() with { x < local::y; });
    void'(std::randomize(x, y) with { x < y; });
  end
endmodule
