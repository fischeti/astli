// At a definition's defaults, a write through an interface port or
// instance is to the variable of the interface instance it names, and each
// element of an array of them is an instance of its own. Writes it cannot
// place on one instance, through an index it cannot evaluate or through two
// ports that may be connected to one instance, are not compared.
interface bus_if;
  logic       valid;
  logic [7:0] data;
  modport src (output valid, output data);
endinterface

module elements (bus_if.src p [2]);
  assign p[0].valid = 1'b0;
  assign p[1].valid = 1'b1;
endmodule

module element_twice (bus_if.src p [2]);
  assign p[0].valid = 1'b0;
  assign p[0].valid = 1'b1;
  assign p[1].data[3:0] = '0;
  assign p[1].data[7:4] = '1;
endmodule

module looped (bus_if.src p [2]);
  for (genvar i = 0; i < 2; i++) begin : g_elem
    assign p[i].valid = 1'b0;
  end
endmodule

module ports (bus_if.src a, bus_if.src b);
  assign a.valid = 1'b0;
  assign b.valid = 1'b1;
  assign a.data = '0;
  always_comb a.data = '1;
endmodule

module instances;
  bus_if buses [2] ();
  bus_if one ();
  assign buses[0].valid = 1'b0;
  assign buses[1].valid = 1'b1;
  assign one.valid = 1'b0;
  assign one.valid = 1'b1;
endmodule
