// A `bind` into the instance a path names takes its names from that
// instance's definition, as one naming the definition does; so do one into
// chosen instances and one naming an instance alone. A `defparam`'s path
// names a parameter below.
module target #(parameter int N = 1);
  logic q;
endmodule

module chk (input logic a);
endmodule

module top;
  logic p;
  if (1) begin : g
    target u_t ();
  end
  bind g.u_t chk u_one (.a(q));
  bind target : g.u_t chk u_two (.a(q));
  defparam g.u_t.N = 2;
  target u_n ();
  bind u_n chk u_three (.a(q));
endmodule
