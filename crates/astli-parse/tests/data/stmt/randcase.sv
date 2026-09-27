// A `randcase` is a `case` with no expression, whose arms' values are
// weights: `(a + b)` is one, not a header.
module m;
  initial begin
    randcase
      3: x = 1;
      (a + b) : begin y = 2; end
      w: ;
    endcase
  end
endmodule
