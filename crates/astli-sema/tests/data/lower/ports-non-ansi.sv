// Non-ANSI ports: the list names them, and the body gives each its
// direction and type, in either order.
module m (a, b, c);
  input a;
  reg [3:0] b;
  output [3:0] b;
  output c;
  wire c;
  logic local_one;
endmodule

// An interface port is declared in the body by its interface's name, with
// or without a modport.
module n (x, y);
  bus x;
  bus.mp y;
endmodule
