// A signal through an interface port is as wide as the interface connected
// to it makes it: directly, through a modport, an array, a port passed on,
// a generic port, `.name` or `.*`.
interface bus_if #(parameter int W = 8);
  logic [W-1:0] data;
  modport producer (output data);
endinterface

module narrow (bus_if bus);
  logic [7:0] low;
  assign low = bus.data;
endmodule

module producer (bus_if.producer bus);
  logic [15:0] wide;
  assign bus.data = wide;
endmodule

module generic (interface bus);
  logic [7:0] low;
  assign low = bus.data[11:0];
endmodule

// A parameter through the port is a constant.
module passed (bus_if bus);
  logic [bus.W/4-1:0] low;
  assign low = bus.data;
endmodule

module passes (bus_if bus);
  passed u_passed (.bus(bus));
endmodule

module elements (bus_if bus [2]);
  logic [7:0] low;
  assign low = bus[1].data;
endmodule

module element (bus_if bus);
  logic [6:0] low;
  assign low = bus.data;
endmodule

module by_name (bus_if bus);
  logic [5:0] low;
  assign low = bus.data;
endmodule

module by_star (bus_if bus);
  logic [4:0] low;
  assign low = bus.data;
endmodule

module top;
  bus_if #(.W(16)) wide ();
  bus_if #(.W(16)) wides [2] ();
  bus_if fits ();
  bus_if #(.W(4)) tiny ();
  // 16 bits into 8.
  narrow u_wide (.bus(wide));
  // 8 bits, which fit.
  narrow u_fits (.bus(fits));
  // 16 bits into 4.
  producer u_producer (.bus(tiny.producer));
  generic u_generic (.bus(wide));
  passes u_passes (.bus(wide));
  elements u_elements (.bus(wides));
  element u_element (.bus(wides[0]));
endmodule

module implicit;
  bus_if #(.W(16)) bus ();
  by_name u_name (.bus);
  by_star u_star (.*);
endmodule

// An interface instance's own signal, and one of another interface than the
// port names, which tells it nothing.
interface other_if;
  logic [31:0] data;
endinterface

module own;
  bus_if #(.W(16)) bus ();
  other_if other ();
  logic [7:0] low;
  assign low = bus.data;
  narrow u_other (.bus(other));
endmodule
