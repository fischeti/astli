// An assignment may be a value, in parentheses and only there; nested, and
// with any operator but `<=`, which in parentheses compares.
module m;
  initial begin
    b = (a -= 1);
    a = (b = (c = 5));
    d = ((b += (a += 1) + 1));
    if ((x = next()) != 0) y = x;
    e = (a <= b);
  end
endmodule
