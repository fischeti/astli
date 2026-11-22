// Each kind of name, declared and used.
package pkg;
  localparam int Depth = 4;
  typedef enum logic [1:0] { Idle, Busy } state_t;
  typedef struct packed { logic valid; logic [7:0] data; } beat_t;
  function automatic int twice(int value);
    return value * 2;
  endfunction
endpackage

interface bus_if;
  logic req;
  modport host (output req);
endinterface

module leaf #(parameter int Width = 8, parameter type T = logic) (
  input logic clk
);
endmodule

module kinds
  import pkg::*;
(
  input logic clk,
  bus_if.host bus
);
  wire ready;
  state_t state;
  pkg::beat_t beat;
  logic [pkg::Depth-1:0] count;
  genvar i;

  leaf #(.Width(pkg::Depth)) u_leaf (.clk);

  task automatic tick();
    count = twice(count);
  endtask

  always_ff @(posedge clk) begin : seq
    state <= Busy;
    if (beat.valid) count <= beat.data;
  end

  assign bus.req = ready;
  assign floating = ready;
endmodule
