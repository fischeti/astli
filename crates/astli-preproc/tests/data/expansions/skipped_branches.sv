// Each branch not taken is skipped, whichever directive opens it, and one
// nested in a skipped branch is inside it.
`define B
`ifdef A
a;
`ifdef B
nested;
`endif
`elsif B
b;
`else
c;
`endif
`ifndef B
d;
`endif
`ifdef B
`else
e;
`endif
