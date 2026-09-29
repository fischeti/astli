// A variable given a value where a module or interface declares it, however
// its type is written, and in a generate construct, which is the module's
// scope still. Only the declarator with a value is reported.
module declarations (
  input logic a_i
);
  typedef struct packed {
    logic a;
    logic b;
  } pair_t;

  logic one = 1'b1;
  var logic [7:0] sum = a_i + a_i;
  pair_t pair = '{a: 1'b1, b: 1'b0};
  logic plain, set = 1'b0;

  if (1) begin : gen_a
    logic held = 1'b0;
  end
endmodule

interface bus_if;
  logic ready = 1'b0;
endinterface
