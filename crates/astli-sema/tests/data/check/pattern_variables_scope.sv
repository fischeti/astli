// A pattern variable is not in scope past what its match guards: not in the
// `else` of an `if`, after the `if`, in another case item, or in the false
// arm of a conditional.
module m;
  typedef union tagged {
    void Invalid;
    int Valid;
  } VInt;

  VInt v;
  int y;

  initial begin
    if (v matches tagged Valid .n) y = n;
    else y = n;
    y = n;
    case (v) matches
      tagged Valid .k : y = k;
      default : y = k;
    endcase
    y = v matches tagged Valid .w ? w : w;
  end
endmodule
