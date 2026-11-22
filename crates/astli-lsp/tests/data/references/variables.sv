// A variable is found from its declaration and from a use, and an
// implicit net, which its first use declares, from a use.
module variables;
  logic x, y;
//      ^
  assign y = x;
//           ^
  assign x = 1'b1;
  variables_sink sink (.d(w));
  assign y = w;
//           ^
endmodule

module variables_sink (input logic d);
endmodule
