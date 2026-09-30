// An instance of a definition no file declares; a connection or override
// by name its definition lacks, or given twice; more by position than it
// has. `.*` and an implicit `.name` count as connections.
module sub #(parameter int W = 1, localparam int L = 2) (input logic a, output logic b);
endmodule

module legacy (a, b);
  parameter P = 1;
  localparam Q = 2;
  input a;
  output b;
endmodule

module m;
  logic a, b, c;
  sub #(.W(2), .X(3)) u_named (.a, .b(b), .c(c), .a(c));
  sub #(1, 2) u_positional (a, b, c);
  sub u_wild (.*);
  legacy #(.P(1), .Q(2)) u_legacy (a, b);
  legacy #(1, 2) u_legacy_positional (.a, .b);
  nowhere u_nowhere (a);
endmodule

module local_override;
  sub #(.L(3)) u_sub (.a(), .b());
endmodule

// An instance in a generate branch may never be built, so its definition
// need not exist; what it connects is checked when it does, as a warning.
module generated #(parameter bit Fast = 0);
  if (Fast) begin : gen_fast
    fast_only u_fast ();
    sub u_sub (.nope());
  end
endmodule
