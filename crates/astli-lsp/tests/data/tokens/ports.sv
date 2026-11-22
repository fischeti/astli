// A port's direction is a modifier, on its declaration and on each use.
module ports (
  input  logic a,
  output logic b,
  inout  wire  c,
  ref    logic d
);
  assign b = a;
  assign c = d;
endmodule

// A port list that names its ports and a body that declares them.
module old_style (x, y);
  input x;
  output y;
  assign y = x;
endmodule
