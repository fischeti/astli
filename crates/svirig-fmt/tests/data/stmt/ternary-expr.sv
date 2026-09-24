// A ternary chain that does not fit ends each condition's arm after its `:`,
// and gives the final value a line of its own, all aligned under the first
// condition. A chain written over several lines stays so; one ternary that
// fits is joined.
module m;
  assign a = b?c:d;
  assign e = sel_q ?
             f : g;
  assign foo = condition_a ? a :
               condition_b ? b : not_a_nor_b;
  assign next_state_d = request_valid_and_granted_q ? StateTransferringPayload : StateWaitingForGrant;
  assign next = start_i ? StIdle : busy_q ? StBusy : done_q ? StDone : error_q ? StError : StFaultDetected;
  logic [(Wide ? 16 : 8)-1:0] x;
endmodule
