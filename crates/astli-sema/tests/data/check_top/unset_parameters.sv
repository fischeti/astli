// A parameter without a default takes its value from every instance built:
// an override, by name or by position, or a `defparam`. One left out is an
// error on the instance, and on the parameter of a top that nothing sets.
// An instance in a branch not built gives nothing, and needs to give
// nothing.
module adder #(
  parameter int unsigned Width,
  parameter type T,
  parameter int Depth = 2
) ();
endmodule

// A `defparam` sema cannot follow may set a parameter, so one of a name a
// `defparam` sets is never told.
module counter #(parameter int Max) ();
endmodule

module wrapper #(parameter int unsigned Width = 4) ();
  adder #(.Width(Width), .T(logic)) u_passed ();
endmodule

module top #(parameter int N) ();
  adder u_none ();
  adder #(.Depth(3)) u_named ();
  adder #(8, logic) u_positional ();
  adder #(, logic) u_empty ();
  counter u_defparam ();
  defparam u_defparam.Max = 8;
  wrapper u_wrapper ();
  if (0) begin : gen_never
    adder u_unbuilt ();
  end
endmodule
