// A cast takes no space around its `'`, nor inside its parentheses.
module m;
  assign a = logic ' ( b );
  assign c = 8 '(d + e);
  assign f = signed'(g);
  assign h = pkg::t_e '(i);
  assign k = state_e'(next_state_candidate_from_the_arbiter_q + offset_value_for_this_channel_q + 1);
endmodule
