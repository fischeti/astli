// An unpacked range runs low to high, or is a size. `[N:0]` counts as
// descending whatever N is; packed ranges are another rule's.
module unpacked_ranges;
  logic [7:0] a [0:3];
  logic [7:0] b [3:0];
  logic [7:0] c [N-1:0];
  logic [7:0] d [4];
  logic [7:0] e [0:N-1];
  logic       f [2][7:0];
endmodule
