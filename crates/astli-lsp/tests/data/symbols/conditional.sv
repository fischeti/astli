// Every branch of a conditional is listed, and folds on its own. A macro
// call standing where an item would is not a symbol, and neither is what
// it is given.
`define REG(name, width) \
  logic [width-1:0] name; \
  always_ff @(posedge clk) name <= name``_d;

`define SIMPLE 1

// A run of comment lines folds,
// as one.
module cond (
  input logic clk,
`ifdef WITH_DEBUG
  input logic dbg,
`endif
  output logic q
);
`ifdef FPGA
  logic fpga_only;
  sub_fpga u_impl ();
`elsif ASIC
  logic asic_only;
  sub_asic u_impl ();
`else
  logic sim_only;
`endif

  `REG(state, 4)
  `REG(other, 8)

  logic after_macros; // a trailing comment
  // does not start a run with the line after it

  /* A block comment
     over two lines folds. */
endmodule
