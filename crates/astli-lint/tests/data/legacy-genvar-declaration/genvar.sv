// A genvar declared on its own is reported, several at once as one; one in
// the loop's header is not.
module genvars;
  genvar i;
  genvar j, k;
  for (i = 0; i < 2; i++) begin : gen_a
  end
  for (genvar m = 0; m < 2; m++) begin : gen_b
  end
endmodule
