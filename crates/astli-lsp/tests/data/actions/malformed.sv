// The parser does not read this file whole, since it has no `endmodule`,
// so `reg` is offered no fix, as `astli lint --fix` leaves such a file.
module malformed;
  reg q;
