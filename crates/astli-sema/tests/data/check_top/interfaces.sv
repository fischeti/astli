// A variable of an interface instance is driven by every body connected to
// it, by the body holding it, and by the interface itself; two of them
// clash where the instance is one. Two instances of one interface share a
// body but not their variables, and neither do two elements of an array
// of them. A read is no driver.
interface bus_if;
  logic [7:0] data;
  logic       valid;
  logic       ready;
  assign ready = 1'b1;
  modport host (output data, output valid);
endinterface

module writer (bus_if.host bus);
  assign bus.data = 8'h0;
endmodule

module low_writer (bus_if bus);
  assign bus.data[3:0] = 4'h0;
endmodule

module high_writer (bus_if bus);
  assign bus.data[7:4] = 4'h0;
endmodule

module reader (bus_if bus, output logic seen);
  assign seen = bus.valid & bus.ready;
endmodule

module valid_writer (bus_if bus);
  assign bus.valid = 1'b1;
endmodule

module ready_writer (bus_if bus);
  always_comb bus.ready = 1'b0;
endmodule

module pass (bus_if bus);
  writer u_writer (.bus);
endmodule

module top;
  logic seen_a, seen_b;
  bus_if bus_a ();
  bus_if bus_b ();
  bus_if bus_c ();
  bus_if bus_d ();
  bus_if buses [2] ();

  // Twice through ports.
  writer u_a1 (.bus(bus_a));
  pass u_a2 (.bus(bus_a));

  // Once each: one instance per interface instance, and readers.
  writer u_b (.bus(bus_b));
  writer u_c (bus_c);
  reader u_rb (.bus(bus_b), .seen(seen_a));
  reader u_rc (.bus(bus_c), .seen(seen_b));

  // Apart in their bits.
  low_writer u_low (.bus(bus_d));
  high_writer u_high (.bus(bus_d));

  // The holder and an instance, and the interface and an instance.
  assign bus_d.valid = 1'b0;
  valid_writer u_valid (.bus(bus_d));
  writer u_d (.bus(bus_d.host));
  ready_writer u_ready (.bus(bus_d));

  // Each element of an array once.
  for (genvar i = 0; i < 2; i++) begin : g_elem
    writer u_e (.bus(buses[i]));
  end
endmodule
