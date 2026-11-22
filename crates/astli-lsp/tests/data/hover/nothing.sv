// A name sema cannot resolve says nothing, nor does a place that holds
// no name, nor a macro no file defines.

module top;
  logic a;
  assign a = missing;
//           ^
  assign a = pkg_nowhere::thing;
//                        ^
  initial begin
//^
  end
  `UNDEFINED
//^
endmodule
