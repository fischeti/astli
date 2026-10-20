// A specify block gives the delays of the paths through a module, `=>` each
// input to its own output and `*>` each to all, under a state or not, and
// checks the timing of its signals. A conditional may choose among them.
// A path's delays may go without parentheses, and a `specparam` may give the
// limits of a pulse.
module dff (input a, b, clk, d, output q, y);
  specify
    specparam tRise = 1:2:3, tFall = 4;
    specparam PATHPULSE$a$y = (1:2:3, 4);
    (a => y) = 10;
    (a, b[0] *> q, y) = (tRise, tFall);
    (a +=> y) = 1;
    (b - *> y) = (1:2:3, 4:5:6);
    (posedge clk => (q +: d)) = (3, 2);
    if (a && !b) (b => y) = 5;
    ifnone (b => y) = 6, 7;
    pulsestyle_onevent q;
    noshowcancelled q, y;
    $setup(d, posedge clk &&& a, 1, notifier);
    $setuphold(posedge clk, d, 1, 2, , , , dclk, dd);
    $width(edge [01, x1, 0z] clk, 5);
`ifdef TIMING
    $period(negedge clk, 10);
`endif
  endspecify
endmodule
