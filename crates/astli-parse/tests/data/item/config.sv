// A configuration names the cells a design starts from, then binds
// instances and cells to libraries, to other cells or configurations, or to
// other parameter values.
config cfg;
  localparam int W = 4;
  design work.top lib2.other;
  default liblist work lib2;
  instance top.u1 liblist lib3;
  instance top.u2 use lib4.alt : config;
  instance top.u3 use #(.W(8));
  cell adder use fast_adder;
  cell lib.mul liblist work;
endconfig : cfg
