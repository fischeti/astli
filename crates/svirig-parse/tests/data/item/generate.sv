// A generate construct is the statement shape over an item body, and a
// conditional one needs no `generate` around it.
module m;
generate
  for (genvar i = 0; i < 2; i++) begin : g
    assign x = i;
  end
endgenerate
  if (W > 1) begin
    assign a = b;
  end else begin
  end
endmodule
