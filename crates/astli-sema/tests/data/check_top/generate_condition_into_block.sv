// A generate condition or loop bound that names a member of a generate
// block beside it is not a constant: deciding it must not elaborate the
// block it is deciding. Neither is a `case` selector, a parameter's value
// or an override naming one; a parameter of the interface a port connects
// is a constant.
interface bus #(parameter int W = 4) ();
endinterface

module sub #(parameter int W = 1) (bus port);
  localparam int V = port.W;
endmodule

module top;
  if (1) begin : blah
    int foo;
    localparam int P = 1;
  end

  genvar i;
  for (i = 0; i < blah.foo; i++) begin : g_loop
  end

  if (blah.foo) begin : g_if
  end

  case (g_case.P)
    0: begin : g_case
      localparam int P = 0;
    end
  endcase

  localparam int Q = blah.P;

  bus b ();
  sub #(.W(blah.P)) u (.port(b));
endmodule
