// A parameter in a `#( ... )` list takes no attribute. Each is an error of its
// own, and the parameter after it parses as if it were absent.
module m #(
  (* sweep = "1, 2" *) parameter int unsigned Depth = 4,
  (* sweep *) parameter bit Mode = 0,
  (* a *) (* b *) Width = 8
) (
  (* keep *) input logic clk
);
  (* sweep = "1, 2" *) parameter int P = 1;
  (* keep *) localparam int Q = 2;
endmodule

interface i #((* a *) W = 1, (* b *) type T = logic);
endinterface

program p #((* a *) parameter int N = 2);
endprogram

class c #((* a *) int N = 2, (* b *) localparam int M = N);
endclass
