// A time literal is a `realtime` in the time unit of the design element it
// is written in, rounded to its precision: what the element's `timeunit`
// and `timeprecision` declare, or the last `` `timescale `` before it, or
// 1ns/1ns. A `time` takes it rounded to an integer, and what is computed
// from it.
module plain;
  localparam time A = 10ns;
  localparam realtime B = 1.4ns;
endmodule
`timescale 1ns/1ps
module scaled;
  localparam realtime C = 1.5ns;
  localparam time D = 2us;
  localparam time E = D * 3 / 4;
  localparam time F = 10ps;
endmodule
module own;
  timeunit 100ps;
  timeprecision 10ps;
  localparam realtime G = 1ns;
  localparam realtime H = 15ps;
endmodule
`resetall
module reset;
  localparam realtime I = 1.5ns;
endmodule
