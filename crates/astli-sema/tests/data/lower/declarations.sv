// Nets, variables, typedefs and the names an enum declares where it is
// written.
module m import pkg::*, other::x; ();
  import third::*;
  wire [3:0] a = 4'h0, b;
  wire signed [1:0] s;
  var logic v;
  logic [7:0] mem [4][];
  int q [$];
  pkg::t_e [1:0] packed_array;
  typedef enum logic [1:0] {IDLE, BUSY = 2'd2} state_e;
  typedef struct packed { logic valid; pkg::t_e [1:0] data; } req_t;
  typedef union packed { logic [1:0] a; logic [1:0] b; } u_t;
  typedef logic [3:0] nibble_t [2];
  state_e state;
  genvar i;
endmodule
