// A constraint ends at its brace rather than at a semicolon. Without the
// rule, the fallback would carry on into the member after it.
class C;
  constraint c { a inside {[0:3]}; }
  int x;
endclass
class D;
  extern constraint c;
endclass
