// `bind` adds an instance at the end of every instance of its target, its
// overrides evaluated there. One into chosen instances is not in the tree,
// whose bodies instances of one definition share.
module sub #(parameter int W = 1) (input logic [W-1:0] d);
endmodule

module chk #(parameter int W = 1) (input logic [W-1:0] d);
endmodule

module target #(parameter int N = 2);
  logic [N-1:0] q;
endmodule

module top;
  target #(.N(4)) u_a ();
  target u_b ();
endmodule

bind target chk #(.W(N)) u_chk (.d(q));
bind target : u_a sub u_sub (.d(q));
bind top.u_b sub u_path (.d(q));
