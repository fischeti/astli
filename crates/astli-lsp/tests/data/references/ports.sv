// A definition is found from its declaration and from an instance, and a
// port and a parameter from theirs and from a connection or an override,
// by name, by `.name` and by position. At `.a` stand the port and the
// name it connects, and both are found.
module ports_child #(parameter int N = 1) (input logic a, output logic b);
//     ^                           ^                   ^
  assign b = a;
endmodule

module ports;
  logic a, y;
  ports_child #(.N(2)) named (.a, .b(y));
//^              ^             ^
  ports_child #(3) positional (a, y);
endmodule
