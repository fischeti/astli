// Not reported: a name meant to be unused, a signal read only through a
// connection, an event control or a region sema does not lower, a signal
// another module reaches by a hierarchical name, a waived one, a package's.
package p;
  logic in_package;
endpackage

module sub (input logic i);
  logic s;
  always_comb s = i;
endmodule

module m (input logic clk_i);
  logic unused_spare, connected, in_event, in_property;
  sub u_sub (.i(connected));
  always @(in_event) ;
  assert property (@(posedge clk_i) in_property);
  (* astli_allow = "unused-signal" *)
  logic waived;
endmodule

module top;
  m u_m (.clk_i(1'b0));
  initial $display(u_m.u_sub.s);
endmodule
