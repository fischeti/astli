// A member of an instance, a generate block, an interface port or a
// definition named from the top is its definition's symbol; a struct's
// member is no name, and one the definition does not declare is unknown,
// since a generate block the HIR does not name may hold it.
interface bus_if;
  logic valid;
  modport host (output valid);
endinterface

module leaf (input logic d);
  logic q;
  if (1) begin : g_inner
    logic deep;
  end
endmodule

module top (bus_if.host bus);
  typedef struct packed { logic a; } s_t;
  s_t s;
  logic v;
  leaf u_leaf (.d(v));
  leaf u_arr [2] (.d(v));
  for (genvar i = 0; i < 2; i++) begin : g_loop
    logic here;
  end
  assign v = u_leaf.q | u_arr[1].q | u_leaf.g_inner.deep | g_loop[0].here
           | top.v | s.a | u_leaf.nothing | bus.valid;
endmodule

module bench;
  virtual bus_if vif;
  typedef struct packed { logic valid; } t_t;
  t_t t;
  initial vif.valid = t.valid;
endmodule
