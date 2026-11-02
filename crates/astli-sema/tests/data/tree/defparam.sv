// A `defparam` overrides the parameter its path names, over the instance's
// own override, and the last one wins. The instances it reaches have
// bodies of their own; one from a top's sibling reaches into the top.
module leaf #(parameter int P = 1, localparam int L = P);
endmodule

module mid #(parameter int Q = 1);
  leaf u_leaf ();
  leaf u_other ();
endmodule

module top #(parameter int T = 1);
  mid u_a ();
  mid u_b ();
  leaf #(.P(5)) u_c ();
  leaf u_arr [1:0] ();
  for (genvar i = 0; i < 2; i++) begin : gen
    leaf u_g ();
  end
  defparam u_a.u_leaf.P = 2, u_c.P = 6;
  defparam u_a.Q = 3, u_a.Q = 4;
  defparam gen[1].u_g.P = 7;
  // Neither a local parameter nor one element of an array is told apart.
  defparam u_c.L = 8, u_arr[0].P = 9;
endmodule

module annotate;
  defparam top.T = 10, top.u_b.u_other.P = 11;
endmodule
