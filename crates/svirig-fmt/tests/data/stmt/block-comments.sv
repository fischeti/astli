// A comment after `begin` on its own line is indented with the statements,
// and one before `end` stays with the last of them.
module m;
  always_comb begin // why
    // first
    a = b;
    // last
  end // done
endmodule
