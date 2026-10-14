// Generate constructs as built: the `if` arm whose condition holds, the
// `case` arm whose label matches, a block per loop iteration with the
// genvar's value inside it; an unnamed block named for its construct's
// place. A condition sema cannot evaluate leaves the construct undecided.
package p;
  localparam int Lanes = 3;
endpackage

module leaf #(parameter int I = 0) ();
endmodule

module top #(parameter int Mode = 2, parameter int Unset) ();
  if (Mode > 4) begin : g_big
    leaf u_big ();
  end else if (Mode > 1) begin : g_mid
    leaf #(.I(Mode)) u_mid ();
  end else begin : g_small
  end

  case (Mode)
    0, 1: begin : g_low leaf u_low (); end
    2: leaf u_two ();
    default: begin : g_other end
  endcase

  for (genvar i = 0; i < p::Lanes; i++) begin : g_lane
    localparam int Doubled = i * 2;
    leaf #(.I(Doubled)) u_lane ();
    for (genvar j = i; j > 0; j -= 1) begin : g_inner
      leaf #(.I(i * 10 + j)) u_inner ();
    end
  end

  genvar k;
  for (k = 0; k < 4; k = k + 2) begin
    leaf #(.I(k)) u_step ();
  end

  if (Unset > 0) begin : g_unknown
    leaf u_maybe ();
  end

  if (0) begin : g_never
    leaf u_never ();
  end
endmodule
