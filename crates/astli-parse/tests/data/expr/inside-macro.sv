// A macro call may write the whole list after `inside`, `dist` or
// `intersect`, with or without arguments, in an expression or a constraint.
class c;
  rand int addr;
  constraint in_range { addr inside `RANGE(Idx); }
  constraint weighted { addr dist `WEIGHTS; }
endclass

module m;
  initial x = a.b inside `P;
endmodule
