// Each case item stands on a line of its own, with its `:` straight after
// the values and a space after it. A comment ending the item's line puts
// its statement on the next, one level in.
module m;
  always_comb begin
    unique case (state_q)
      StIdle   : state_d = StA;
      StA, StB:
        state_d = StIdle;
      StC: begin
        state_d = StIdle;
      end
      StD: // why
        state_d = StIdle;
      default : state_d = StIdle;
    endcase
    case (x) inside
      [0:3]: y = 1;
      default y = 0;
    endcase
  end
endmodule
