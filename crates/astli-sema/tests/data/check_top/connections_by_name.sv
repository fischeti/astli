// A connection by `.name` or by `.*` is typed as one by expression: the
// declaration of the port's name where the instance stands is converted to
// an input's type, and an output's type to it. A parameter with no type of
// its own whose value is text is text.
package p;
  typedef enum logic [1:0] {Idle, Busy} state_e;
endpackage

module sub import p::*; (
  input state_e state_i,
  input string label_i,
  output state_e state_o
);
  assign state_o = state_i;
endmodule

module star import p::*; ();
  logic [1:0] state_i;
  localparam label_i = "idle";
  string state_o;
  sub u_star (.*);
endmodule

module top import p::*; ();
  logic [1:0] state_i;
  localparam label_i = "idle";
  state_e state_o;
  sub u_name (.state_i, .label_i, .state_o);
  star u_wrap ();
endmodule
