// `matches` takes a value apart against a pattern, where `.v` binds `v` and
// `.*` matches anything. It binds tighter than `&&`, and `&&&` joins the
// conditions of an `if`, a `?:` or an arm, looser than `||`.
module m;
  initial begin
    if (x matches tagged Valid .n && n > 0) a = n;
    if (x matches tagged Pair '{.a, .*} &&& a != 0 || b) a = 1;
    y = x matches tagged Add '{src: .s, dst: 0} ? s : 0;
    case (instr) matches
      tagged Jmp (tagged JmpU .t) : pc = t;
      tagged Add '{.r1, .r2} &&& r1 != 0 : f(r1, r2);
      .* : ;
      default : ;
    endcase
    // Outside a pattern, a `.` opens nothing.
    case (x)
      0 : ;
    endcase
  end
endmodule
