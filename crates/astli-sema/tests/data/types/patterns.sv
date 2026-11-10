// A pattern variable has the type of what it matches: a tagged union's
// member, a struct's by name or by position.
module m;
  typedef union tagged {
    void Invalid;
    int Valid;
  } VInt;

  typedef union tagged {
    struct packed {
      bit [4:0] reg1;
      logic [2:0] regd;
    } Add;
    bit [9:0] Jmp;
  } Instr;

  struct {int a; real b;} s;
  VInt v;
  Instr instr;

  function automatic void f();
    int i;
    real r;
    logic [2:0] l;
    bit [4:0] b;
    bit [9:0] p;
    if (v matches tagged Valid .k) i = k;
    if (s matches '{a: .*, b: .q}) r = q;
    if (instr matches tagged Add '{.r1, .d}) l = d;
    // A pattern variable is matched in turn.
    if (instr matches tagged Add .a &&& a matches '{reg1: .r1}) b = r1;
    case (instr) matches
      tagged Jmp .j : p = j;
    endcase
    i = v matches tagged Valid .k ? k : 0;
  endfunction
endmodule
