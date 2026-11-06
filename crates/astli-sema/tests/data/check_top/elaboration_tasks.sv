// An elaboration system task runs where its scope is built: a `$fatal` or
// an `$error` is an error, a `$warning` a warning, each saying what its
// arguments format to in that body; an `$info` says nothing wrong. A
// branch not built runs none, and two instances with the same parameters
// say it once.
module sub #(parameter int W = 8, parameter string NAME = "sub") ();
  if (W != 4) begin : gen_err
    $error("W must be 4, is %0d", W);
  end
  if (W > 16) begin : gen_wide
    $fatal(1, "W is too wide");
  end
  for (genvar i = 0; i < 2; i++) begin : gen_loop
    if (i == 1) $warning("%s: iteration %0d", NAME, i);
  end
  $info("built with W=%0d", W);
endmodule

module top;
  sub #(.W(4)) u_ok ();
  sub #(.W(5)) u_odd ();
  sub #(.W(5)) u_odd_again ();
  sub #(.W(32), .NAME("wide")) u_wide ();
  if (1) $error;
  if (0) $fatal;
endmodule
