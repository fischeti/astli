// An instance's definition goes to the definition, a formal connected or
// overridden by name to the formal, and an implicit `.name` to the formal
// and to the name. A positional connection or override goes to the formal
// it lands on only from where no name inside it stands.
module child #(parameter int N = 1) (input logic a, output logic b);
  assign b = a;
endmodule

module instances;
  logic a, y;
  child #(.N(2)) named (.a, .b(y));
//^        ^             ^   ^ ^
  child #(1 + 2) positional (a | y, y);
//          ^                ^ ^
endmodule
