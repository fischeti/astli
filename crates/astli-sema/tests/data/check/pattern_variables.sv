// Not reported: a pattern variable, `.name` in a `matches` pattern, is
// declared by the pattern, and is in scope in what its match guards: the
// rest of a `&&&` chain, the true arm of a conditional, the body of an `if`
// and the blocks inside it, and a `case` item's statement.
module m;
  typedef union tagged {
    void Invalid;
    int Valid;
  } VInt;

  typedef union tagged {
    struct {
      bit [4:0] reg1, reg2, regd;
    } Add;
    bit [9:0] Jmp;
  } Instr;

  struct {int a; real b;} s;
  int i;
  VInt v;
  Instr instr;
  int rf[32];
  bit [9:0] pc;

  function automatic int f(VInt u);
    return u matches tagged Valid .x &&& x > 2 ? (u matches tagged Valid .w ? x + w : 1) : 2;
  endfunction

  initial begin
    static int b = s matches '{a:.p, b:.q} ? p : 0;
    if (i matches .n) begin
      begin
        automatic int c = n;
      end
    end
    case (v) matches
      tagged Invalid : $display("invalid");
      tagged Valid .k : $display("valid %d", k);
    endcase
    case (instr) matches
      tagged Add '{.r1, .r2, .rd} &&& (rd != 0) : rf[rd] = rf[r1] + rf[r2];
      tagged Jmp .j : pc = pc + j;
    endcase
  end
endmodule
