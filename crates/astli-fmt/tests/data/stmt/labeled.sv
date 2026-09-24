// A label stands against its `:`, and the statement it names follows on the
// same line.
module m;
  initial begin
    first  :  x = 1;
    second:begin
      y = 2;
    end
  end
endmodule
