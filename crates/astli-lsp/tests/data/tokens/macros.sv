// A macro call is a macro; a name in its argument is what it names, and a
// name its body writes is what it is in every expansion.
`define TWICE(x) ((x) + (x))
`define READ_W (w)
`define DECLARE(name) logic name;

module macros;
  logic w;
  localparam int P = 3;
  wire [7:0] sum = `TWICE(P);
  wire [7:0] read = `READ_W;
  `DECLARE(made)
  initial $display("%d", `__LINE__, made);
endmodule
