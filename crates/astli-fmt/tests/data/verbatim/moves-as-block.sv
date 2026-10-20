// What no rule lays out keeps its lines, and they move with the first, as
// an `ifdef inside an expression does.
// A line that starts inside a token stays where it was.
module m;
        assign s = sel ?
          /* over
             lines */
          "a \
  string" :

`ifdef B
            b;
`else
    // further left than the first line, which stops at the margin
            c;
`endif
endmodule
