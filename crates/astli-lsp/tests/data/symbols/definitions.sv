// Each kind of definition, and what a module declares.
module top #(
  parameter int W = 8,
  localparam int D = W * 2
) (
  input  logic         clk,
                       rst_n,
  output logic [W-1:0] q,
  bus_if.master        bus
);
  typedef enum logic [1:0] {IDLE, BUSY = 2'd2} state_t;
  typedef struct packed {
    logic       valid;
    logic [7:0] data;
  } beat_t;
  typedef logic [W-1:0] word_t;

  wire  a, b;
  logic [D-1:0] count = '0;
  state_t state;

  sub #(.W(W)) u_sub (.clk, .q(a)), u_other ();
  and g_and (a, b, clk);

  function automatic int add(input int x, y);
    int sum;
    sum = x + y;
    return sum;
  endfunction

  task automatic wait_cycles(int n);
    repeat (n) @(posedge clk);
  endtask
endmodule : top

interface bus_if;
  logic valid;
  modport master (output valid);
  modport slave (input valid);
endinterface

program automatic test_prog;
  initial $display("hi");
endprogram

package pkg;
  parameter int N = 4;
  function void helper();
  endfunction
endpackage

class Packet extends Base;
  rand bit [7:0] payload;
  function new();
    super.new();
  endfunction
  extern virtual function void show();
  task run();
  endtask
endclass

function void Packet::show();
endfunction

checker c_stable (logic sig, event clk);
  property p_stable;
    @clk $stable(sig);
  endproperty
endchecker

primitive mux (out, sel, a, b);
  output out;
  input sel, a, b;
  table
    0 1 ? : 1;
    1 ? 0 : 0;
  endtable
endprimitive
