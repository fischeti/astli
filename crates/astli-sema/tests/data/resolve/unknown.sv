// What may be declared where sema cannot see is unknown rather than
// undeclared: the head of a hierarchical name, a task called by name up
// the instance tree, an assignment pattern's key, a class's member, a name
// in an opaque region. A module's name is found past the file's scope.
module m (axi_if.mst bus);
  sub u_sub ();
  class C;
    int k;
  endclass
  logic [7:0] v;
  typedef struct packed { logic a; logic b; } s_t;
  s_t s;
  initial begin
    v = u_sub.q + top.u.x + up.z + bus.valid;
    tick(v);
    s = '{a: 1'b1, default: '0};
    v = C::k;
    v = $unit::nope;
    assert property (@(posedge clk) v);
  end
endmodule

module top;
endmodule
