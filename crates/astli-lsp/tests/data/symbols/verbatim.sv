// What the parser could not read is a `VERBATIM` region, and nothing in it
// is listed; what follows it is.
module m;
  logic first;
  assign a = ;
  logic last;
  initial begin
    e = ;
  end
endmodule

// A module missing its `endmodule` falls back whole, so nothing in it is
// listed.
module half;
  logic lost;
