// A declaration, and the comment above it, are cut at eight and six lines,
// the comment keeping those nearest the declaration.

// Line 1 of what the bus does.
// Line 2 of what the bus does.
// Line 3 of what the bus does.
// Line 4 of what the bus does.
// Line 5 of what the bus does.
// Line 6 of what the bus does.
// Line 7 of what the bus does.
// Line 8 of what the bus does.
// Line 9 of what the bus does.
// Line 10 of what the bus does.
// Line 11 of what the bus does.
// Line 12 of what the bus does.
module bus (
  input logic p0,
  input logic p1,
  input logic p2,
  input logic p3,
  input logic p4,
  input logic p5,
  input logic p6,
  input logic p7,
  input logic p8,
  input logic p9,
  input logic p10,
  input logic p11
);
endmodule

module top;
  bus u_bus (.*);
//^
endmodule
