// Each element of an array of interface instances is an instance of its
// own, and an array port connected to one matches its elements left index
// to left index, whatever the bounds of each; an element of it, or all of
// it, may be passed on to a port below.
interface bus_if;
  logic valid;
  modport src (output valid);
endinterface

module elements (bus_if.src p [2]);
  assign p[0].valid = 1'b0;
  assign p[1].valid = 1'b1;
endmodule

module element_twice (bus_if.src p [2]);
  assign p[0].valid = 1'b0;
  assign p[0].valid = 1'b1;
endmodule

module looped (bus_if.src p [2]);
  for (genvar i = 0; i < 2; i++) begin : g_elem
    assign p[i].valid = 1'b0;
  end
endmodule

module looped_and_one (bus_if.src p [2]);
  for (genvar i = 0; i < 2; i++) begin : g_elem
    assign p[i].valid = 1'b0;
  end
  assign p[1].valid = 1'b1;
endmodule

module reversed (bus_if.src p [1:0]);
  assign p[1].valid = 1'b0;
endmodule

module writer (bus_if.src bus);
  assign bus.valid = 1'b0;
endmodule

module passed (bus_if.src p [2]);
  elements u_elements (.p);
  writer u_writer (.bus(p[1]));
endmodule

module top;
  bus_if a [2] ();
  bus_if b [2] ();
  bus_if c [2] ();
  bus_if d [2] ();
  bus_if e [0:1] ();
  bus_if f [2] ();

  elements u_a (.p(a));
  element_twice u_b (.p(b));
  looped u_c (.p(c));
  looped_and_one u_d (.p(d));

  // `p[1]` is `e[0]`.
  reversed u_e (.p(e));
  assign e[0].valid = 1'b1;
  assign e[1].valid = 1'b1;

  passed u_f (.p(f));
endmodule
