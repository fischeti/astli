// A delay in parentheses is a list of values, each `min:typ:max` or one
// value; a net's or a gate's may give three, for rise, fall and turn-off.
module m;
  initial begin
    #(100:200:300) $display("done");
    #(WAIT) x = 1;
  end
  assign #(1:2:3, 4:5:6) a = b;
  assign #(1, 2, 3) c = d;
endmodule
