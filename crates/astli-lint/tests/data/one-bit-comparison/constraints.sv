// In a constraint, `a == 1'b0` states the value the solver is to pick, which
// `!a` would hide, in a `constraint` block or a `randomize() with` alike. An
// array method's `with` is an expression, and is reported.
class constraints;
  rand bit a;
  rand bit b;
  bit      q[$];

  constraint a_c {
    a == 1'b0;
    if (b != 1'b1) a == 1'b1;
  }

  function void f();
    void'(randomize() with { b == 1'b1; });
    q = q.find with (item == 1'b1);
  endfunction
endclass
