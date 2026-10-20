module m;
  // Delays and the signals of an event list, between commas.
  assign #( 1,2 ) a = b;
  assign #(1:2:3 , 4:5:6) c = d;
  always @( x,y ) e = x;
endmodule
