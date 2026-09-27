// A constraint block has one constraint per line, as `begin` and `end` do.
// Its `{` stays on the line of the `if`, `foreach` or `->` it belongs to, and
// `} else` shares a line.
class C;
constraint a_c { x < 4;   y   ==  1; }
  constraint b_c {
      if (m) { a == 1; } else if (n) a == 2; else { unique {p, q}; }
    foreach (arr[i])   { arr[i] < 10; }
  solve a,b before c;   // order
    x ->   { y == 1; }
    soft z == 0;
  }
  constraint e_c {}
  extern   constraint p_c;
endclass
constraint   C::p_c { a == b; }
