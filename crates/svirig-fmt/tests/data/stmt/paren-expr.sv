// An expression in parentheses has no space inside them; an event list keeps
// its own.
module m;
  foo u_foo (.a ( a  ), .b(  ));
  always_ff @( posedge clk ) if ( en ) q <= d;
endmodule
