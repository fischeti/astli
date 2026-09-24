// A region whose branches hold halves of a construct still puts its
// directives at the margin; the halves are written as they were.
module m;
  `ifdef SYN
    if (a) begin
  `else
    if (b) begin
  `endif
    end
endmodule
