// A region written on one line keeps the rest of the line when its keywords
// go, with one blank between what stood either side of each.
module inline_region;
  generate if (1) begin : gen_a end endgenerate // kept
endmodule
