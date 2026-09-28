// Time units stand in a design unit's body, or ahead of any unit, and may
// share a line or come from a macro.
timeunit 1ns / 1ps;

module m;
  timeunit 1ns; timeprecision 10ps;
endmodule

package p;
  timeunit `UNIT;
endpackage
