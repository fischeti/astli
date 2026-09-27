// A `randcase` is laid out as a `case` is.
module m;
  initial begin
    randcase
    3 :x = 1;
      (a+b) : begin y = 2; end
    endcase
  end
endmodule
