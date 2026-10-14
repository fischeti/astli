// Each declaration's type: vectors, packed arrays, packed and unpacked
// structs, enums by name, unpacked arrays, type parameters, a typedef of a
// typedef; and the type each side of an assignment has by itself, through
// selects and members.
package p;
  typedef enum logic [1:0] {Idle, Busy} state_e;
  typedef struct packed {
    logic valid;
    logic [7:0] data;
  } req_t;
endpackage

module m #(
  parameter int Width = 8,
  parameter type data_t = logic [Width-1:0]
) (
  input logic clk,
  input p::req_t req,
  output data_t out
);
  typedef struct {
    int count;
    p::state_e state;
  } stats_t;
  typedef p::req_t reqs_t [4];

  wire [Width-1:0] a;
  logic signed [3:0][7:0] b;
  p::state_e state;
  stats_t stats;
  reqs_t queue;
  real r;
  int c [2][3];
  logic [7:0] d [$];

  assign out = req.data;
  assign a = b[1] + queue[2].data;
  assign a = {req.valid, b[0][6:0]};
  assign r = stats.count * 1.5;
  assign b[3] = c[1][2];
endmodule
