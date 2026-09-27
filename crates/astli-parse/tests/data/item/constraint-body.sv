// Every form a constraint takes. An implication, an `if` and a `foreach`
// take constraints as their arms, and braces hold constraints only if there
// is a `;` inside: `{a, b} == 2'b01` is a concatenation. A range may be a
// value and its tolerance.
class C;
  constraint a_c { soft x dist {0 := 1, [1:3] :/ 2}; y inside {[0:3]}; }
  constraint b_c {
    if (m) { a == 1; b -> c; } else if (n) a == 2; else { unique {p, q}; }
    foreach (arr[i]) { if (i > 0) { arr[i] > arr[i - 1]; } }
    solve a, b before c;
    x -> { y == 1; z == 0; }
    x -> y -> z;
    {a, b} == 2'b01;
    disable soft x;
    x <-> y;
  }
  constraint :initial :final o_c { x inside {[10 +/- 2], [100 +%- 5]}; }
  static constraint s_c;
  pure constraint p_c;
endclass
constraint C::s_c { a == b; }
