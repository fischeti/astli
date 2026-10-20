// A checker's formals may be untyped, a sequence or a property, and default
// to a sequence; its body holds what a module may, and checks with it. An
// event may be passed to one, and it may be instantiated in a procedure or
// from a package.
checker handshake (logic clk, untyped req, sequence ack = req ##1 req,
                   input bit en = 1'b1, output bit ok = 1'b0);
  default clocking @(posedge clk); endclocking
  rand bit choice;
  a_ack: assert property (en |-> ack);
  always_ff @(posedge clk) ok <= req;
endchecker : handshake

package checks;
  checker stable (event clk, logic sig);
  endchecker
endpackage

module m (input logic clk, req);
  handshake u_hs (clk, req);
  checks::stable u_stable (posedge clk, req);
  always @(posedge clk) begin
    if (req) handshake u_in_proc (clk, req);
  end
endmodule
