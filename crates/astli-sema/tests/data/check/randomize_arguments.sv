// Not reported: the names `randomize` takes as arguments, and those a
// constraint after `with (...)` uses, are looked up in the class of the
// object randomized, its inherited members included; a name the list after
// `with` leaves out is looked up where the call is.
class A;
  rand int x;
  rand int y;
  int v;
  static int s;
endclass

class B extends A;
  rand int z;
endclass

class C;
  function void f(A a, B b);
    int y = 5;
    void'(a.randomize() with (x) { x < y; });
    void'(a.randomize(v, x) with { solve v before x; });
    void'(a.randomize(v) with { disable soft v; });
    void'(a.randomize(v, x) with { unique {v, x}; });
    void'(b.randomize(x, y, s, z));
  endfunction
endclass
