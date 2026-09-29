// `reg` is reported wherever it declares, a port or a variable; `logic` and
// `wire` are not.
module regs (
  input  logic a_i,
  output reg   b_o
);
  reg   c;
  logic d;
  wire  e = a_i;
endmodule
