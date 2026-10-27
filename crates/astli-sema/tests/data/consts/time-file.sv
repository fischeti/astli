// A `timeunit` at the file's top level is that of a design element no
// `` `timescale `` precedes; one the element declares wins over a
// `` `timescale ``.
timeunit 1ps / 1ps;
module m;
  localparam time A = 1ns;
endmodule
`timescale 1us/1ns
module n;
  timeunit 1ns / 1ps;
  localparam realtime B = 1.5ns;
endmodule
