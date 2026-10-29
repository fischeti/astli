// A delay is each value it is written with, rise, fall and turn-off; `new
// obj` copies the object `obj` holds.
module m;
  parameter int Rise = 2, Fall = 3;
  wire y;
  logic a;
  assign #(Rise, Fall) y = a;
  assign #(Rise, Fall, 1) y = a;
  initial #(Rise) a = 1;
endmodule

class c;
  int x;
  function c copy();
    copy = new this;
  endfunction
endclass
