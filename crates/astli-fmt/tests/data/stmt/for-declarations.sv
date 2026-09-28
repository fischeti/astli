// Declarations of several types in a `for` header are spaced like one.
module m;
  initial begin
    for (int i=0,state_e s=s.first();i<s.num();i+=1,s=s.next()) x = i;
    for (var int i = 1 , bit c = 1'b0; i < 4; i++) x = c;
  end
endmodule
