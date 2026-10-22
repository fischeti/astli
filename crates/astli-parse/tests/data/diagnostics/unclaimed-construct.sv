// A construct no rule claims, here a malformed one, is a recovery, not a
// complaint. If this reports, the parser is complaining about itself.
module m;
  + a b;
endmodule
