// `bind` adds an instance at the end of every instance of its target, its
// overrides evaluated there. One into chosen instances, by a list or a
// path, adds it to those alone, which then have bodies of their own; one
// whose definition the instance is not of adds nothing. A name alone is a
// definition, or an instance where no definition is so named.
module sub #(parameter int W = 1) (input logic [W-1:0] d);
endmodule

module chk #(parameter int W = 1) (input logic [W-1:0] d);
endmodule

module target #(parameter int N = 2);
  logic [N-1:0] q;
endmodule

module mid;
  target u_x ();
  target u_y ();
endmodule

module top;
  target #(.N(4)) u_a ();
  target u_b ();
  target u_c ();
  mid u_mid ();
  if (1) begin : gen
    target u_g ();
    // A path from a scope around the `bind`.
    bind target : u_c sub u_out (.d(q));
  end
  bind target : u_a, u_mid.u_x sub u_list (.d(q));
  bind target : u_b sub #(.W(N)) u_listed (.d(q));
  bind gen.u_g sub u_block (.d(q));
  bind sub : u_b chk u_wrong (.d(q));
  bind u_c chk u_named (.d(q));
endmodule

bind target chk #(.W(N)) u_chk (.d(q));
// A path from a top.
bind top.u_mid.u_y sub u_absolute (.d(q));
