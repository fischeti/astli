// A match reads what it matches and writes each pattern variable; `.*`
// writes nothing.
module m;
  typedef union tagged {
    void Invalid;
    int Valid;
  } VInt;
  VInt v;
  int y;
  initial begin
    if (v matches tagged Valid .n) y = n;
    case (v) matches
      tagged Valid .* : y = 0;
    endcase
  end
endmodule
