// A tagged union's value is spaced like a keyword and its operands.
module m;
  initial begin
    a = tagged   Invalid;
    b = tagged Valid(42);
    c = tagged Jmp(tagged JmpU   239);
  end
endmodule
