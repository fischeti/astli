config cfg;
localparam int W=4;
  design   work.top lib2.other;
default liblist work   lib2;
  instance top.u2 use lib4.alt:config;
  instance top.u3 use #( .W(8) );
  cell lib.mul liblist work;
endconfig:cfg
