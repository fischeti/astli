// A net or a continuous assignment may say how strongly it drives, and a
// `trireg` the charge it holds.
wire (strong0, weak1) w = 1'b0;
trireg (small) t;
assign (weak0, pull1) x = 1'b1;
assign (highz1, strong0) #1 y = 1'b1, z = 0;
