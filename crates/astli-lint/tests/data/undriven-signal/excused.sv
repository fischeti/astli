// Not reported: a signal written through a hierarchical name from another
// module, one passed to a port without a direction, one a region sema does
// not lower may write, one only `$readmemh` writes.
interface bus_if;
  logic valid;
  modport mst (output valid);
endinterface

module m (bus_if.mst bus);
  logic from_afar, in_region;
  logic [7:0] mem [4];
  initial $readmemh("init.hex", mem);
  always_comb if (from_afar && in_region && mem[0][0]) ;
  initial assert property (in_region);
endmodule

module top;
  bus_if bus ();
  m u_m (.bus);
  initial u_m.from_afar = 1'b1;
endmodule
