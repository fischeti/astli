// Raw mode keeps every branch: it cannot know what a build will define. The
// `endif closes the region, not the last branch.
`ifdef A
a <= 1;
`elsif B
b <= 2;
`else
c <= 3;
`endif
