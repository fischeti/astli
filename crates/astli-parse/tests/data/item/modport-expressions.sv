// A modport port may be an expression given a name, `.name(expr)`, or a name
// with no expression at all, beside plain ports in one list.
interface bus;
  logic [3:0] b;
  localparam int B = 2;
  modport m (input b, .x(b[B:0]), output .y(b[3]), .z());
endinterface
