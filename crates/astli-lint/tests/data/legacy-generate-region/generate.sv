// A `generate` region is reported at its keyword; a loop without one is
// already a generate construct.
module generate_region;
  generate
    for (genvar i = 0; i < 2; i++) begin : gen_a
    end
  endgenerate
  for (genvar i = 0; i < 2; i++) begin : gen_b
  end
endmodule
