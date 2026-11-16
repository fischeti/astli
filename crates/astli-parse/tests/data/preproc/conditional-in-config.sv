// A branch inside a configuration holds what the configuration does: rules,
// and local parameters.
config cfg;
  design work.top;
`ifdef FAST
  localparam int W = 8;
  instance top.u1 use fast_adder;
`else
  default liblist work;
`endif
endconfig
