// Each branch opens a `begin` that the text after the `endif closes. Bounding
// the branch stops the first one taking the second's text.
`ifdef SYN
  always_comb begin
`else
  always_ff begin
`endif
  x <= 1;
end
