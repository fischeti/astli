// An assignment in parentheses is spaced like one written as a statement.
module m;
  initial begin
    b = (a-=1);
    a = ( b=(c =5) );
    if ((x=next())!=0) y = x;
  end
endmodule
