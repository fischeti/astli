// A type as an operand is spaced like any other.
module m;
  initial begin
    case (type(T))
      type(logic[11:0]) : ;
    endcase
    if (type(T)==type(int)) $stop;
  end
endmodule
