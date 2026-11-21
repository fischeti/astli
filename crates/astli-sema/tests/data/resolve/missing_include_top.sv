// An include not found outside every definition may have declared anything
// at the top of the file, which every scope of the file looks in last: a
// name found nowhere is unknown throughout it, and so is one of `$unit`.
module m;
  logic x;
  assign x = FROM_HEADER + $unit::ALSO;
endmodule

`include "generated.svh"
