// A case item's values take a `:` after them, and only `default` may go
// without one. An item that lacks it is an error where the `:` belongs, and
// what follows is still its statement, so it is the one error.
module m;
  initial begin
    case (x)
      a b: ;
      c, d e = 1;
      f <= g;
      default h = 2;
    endcase
    case (x) matches
      tagged Valid .v v = 1;
      default: ;
    endcase
    case (x) inside
      [0 : 3] y = 1;
      default ;
    endcase
    randsequence (main)
      main : case (x) 0 first; 1: second; default third; endcase;
      first : { };
      second : { };
      third : { };
    endsequence
  end

  case (P)
    0 begin : g0
    end
    default begin : g1
    end
  endcase
endmodule
