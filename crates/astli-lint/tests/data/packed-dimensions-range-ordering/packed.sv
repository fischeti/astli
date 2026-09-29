// A packed range runs high to low. `[0:N]` counts as ascending whatever N
// is; two expressions cannot be compared, and unpacked ranges are another
// rule's.
module packed_ranges;
  logic [7:0]       a;
  logic [0:7]       b;
  logic [0:N-1]     c;
  logic [N-1:0]     d;
  logic [M:N]       e;
  logic [3:0][0:1]  f;
  logic             g [0:7];
endmodule
