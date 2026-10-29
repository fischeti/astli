// A class's parameters, base and members, and a method or constraint
// defined outside it, which lands in its scope. A constraint block is
// lowered as the expressions it holds.
class base #(type T = int, int N = 2);
  T items [N];
  function new();
  endfunction
endclass

interface class shape;
  pure virtual function int area();
endclass

class square extends base #(byte, 4) implements shape;
  local int side;
  rand bit [3:0] scale;
  constraint scale_c { scale inside {[1:8]}; scale != side; }
  extern constraint even_c;
  extern virtual function int area();
endclass

constraint square::even_c { scale % 2 == 0; }

function int square::area();
  return side * side;
endfunction
