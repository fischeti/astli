// The formals of a property, a sequence or a `let` may be untyped, a
// sequence or a property, and default to a sequence. A sequence's may be
// `local`, a variable of its own.
module m;
  sequence s_req (untyped a, b = a);
    a ##1 b;
  endsequence
  property p_ack (sequence req, property ack = req |-> 1, logic en = 1'b1);
    en |-> req ##1 ack;
  endproperty
  sequence s_count (local input int n = 0);
    (1, n++) [*3];
  endsequence
  assert property (p_ack(s_req(posedge clk)));
  let pick (untyped x, y = 0) = x ? y : 1;
endmodule
