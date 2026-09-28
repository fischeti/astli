// A pattern is spaced like an expression, but `.v` and `.*` are one word.
module m;
  initial begin
    if (x   matches tagged Valid .n&&&n>0) a = n;
    y = x matches tagged Add '{ .s , . * } ? s : 0;
    case (instr) matches
      tagged Jmp(tagged JmpU .t): pc = t;
      tagged Add '{.r1, .r2} &&& r1 != 0: f(r1, r2);
      default: ;
    endcase
  end
endmodule
