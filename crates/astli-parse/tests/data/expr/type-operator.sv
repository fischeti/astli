// `type(...)` names a type, or the type of an expression. It may stand as a
// type parameter's default, and as an operand: compared, or matched by a
// `case`.
module m #(parameter type T = type(logic [11:0]), parameter type U = type(a + b));
  initial begin
    case (type(T))
      type(logic [11:0]): ;
      default: ;
    endcase
    if (type(T) == type(int) || type(T) !== type(U)) $stop;
  end
endmodule
