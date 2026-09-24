// A `do` loop's `while` follows the `end` of its block, or starts a line of
// its own after a statement or a labelled `end`.
module m;
  initial begin
    do begin
      i++;
    end
    while(i < 4) ;
    do i++; while (i < 8);
    do begin : loop
      j++;
    end : loop while (j < 4);
  end
endmodule
