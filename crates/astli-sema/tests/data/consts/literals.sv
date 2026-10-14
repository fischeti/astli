// Literals as written: an unsized decimal is an `int`, truncated if too
// large; an unsized based one is as wide as its value needs, at least 32
// bits; a sized one is padded with an `x` or `z` that leads it, and
// truncated from the left. An unbased unsized literal fills its context.
module m;
  localparam A = 12;
  localparam B = 4294967295;
  localparam C = 'hff;
  localparam D = 'h1_0000_0000_0;
  localparam E = 'sd5;
  localparam F = 'hx;
  localparam G = 8'hFF;
  localparam H = 8'sb1111_1111;
  localparam I = 4'bx1;
  localparam J = 4'b1z;
  localparam K = 12'o7;
  localparam L = 3'd9;
  localparam M = 6'd?;
  localparam N = 1.5;
  localparam O = 2e3;
  localparam P = "hi";
  localparam string Q = "a\tb";
  localparam logic [7:0] R = '1;
  localparam logic [7:0] S = 'z;
  localparam T = '1;
  localparam int U = 'x;
  localparam integer V = 'x;
  localparam logic [15:0] W = "a";
  localparam [3:0] X = 8'hA5;
endmodule
