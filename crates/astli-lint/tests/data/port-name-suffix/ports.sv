// A port's suffix says its direction, `n` or `p` allowed before it. A port
// without a direction takes the one before it's, unless it has a type of
// its own, which may be an interface's. A function's arguments are not
// ports.
module ports (
  input  logic clk_i,
  input  logic rst_ni,
  input  logic data,
  input  logic valid_i, ready,
  output logic lvds_po,
  output logic lvds_no,
  output logic done_i,
  inout  wire  pad_io,
  inout  wire  pad,
  bus_if.slv   bus
);
  function automatic logic f(input logic a);
    return a;
  endfunction
endmodule

module old_style (a, b_o);
  input  a;
  output b_o;
endmodule
