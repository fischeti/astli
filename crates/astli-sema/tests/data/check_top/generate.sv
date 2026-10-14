// From a top, an instance in a generate block is what the tree makes it:
// in a block built, its definition must exist and its connections fit; in
// one never built, it is not checked; under a construct sema cannot decide,
// or in a definition no top builds, it is warned about as without a top.
module leaf (input logic a);
endmodule

module mid #(parameter bit Fast = 1, parameter int Width) ();
  if (Fast) begin : g_fast
    leaf u_built (.a(1'b0), .b(1'b1));
    absent_built u_absent ();
  end else begin : g_slow
    leaf u_never (.a(1'b0), .b(1'b1));
    absent_never u_absent ();
  end
  if (Width > 0) begin : g_unknown
    leaf u_maybe (.a(1'b0), .b(1'b1));
  end
endmodule

module unreached;
  if (1) begin : g
    leaf u_unreached (.a(1'b0), .b(1'b1));
  end
endmodule

module top;
  mid u_mid ();
endmodule
