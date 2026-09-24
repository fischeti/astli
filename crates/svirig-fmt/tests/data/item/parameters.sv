// Consecutive parameters line up in four columns: the keyword, the type, the
// name and the `=`, then a trailing comment. One without a type lines its name
// up with the others'.
module modname #(
  parameter int Depth = 2048, // 8kB default
  localparam int Aw = $clog2(Depth), // derived parameter
  parameter type req_t = logic,
  parameter SimInit = "none",
  int unsigned NoKeyword = 1
) ();
endmodule

package p;
  localparam int unsigned INTERFACE_WIDTH = 64;  // Bits
  localparam int unsigned INTERFACE_WIDTH_BYTES = (INTERFACE_WIDTH + 7) / 8;
  localparam logic [3:0] Bar = 4'd4;
  parameter key_t Key = `KEY({
    A,
    B
  });
  logic x;
  localparam int After = 1;
endpackage
