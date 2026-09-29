// A `begin` directly inside `generate` is reported; one that is a loop's
// body inside it is a generate block.
module region;
  generate
    begin : grouped
    end
    for (genvar i = 0; i < 2; i++) begin : gen_loop
    end
  endgenerate
endmodule
