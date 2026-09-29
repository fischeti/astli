// A generate block's label starts with `gen_` or `g_`, wherever it is
// written; an unlabelled block is another rule's.
module prefix #(parameter bit P = 1) ();
  if (P) begin : gen_a end
  if (P) begin : g_b end
  if (P) begin : c_blk end
  for (genvar i = 0; i < 2; i++) begin : loop end
  if (P) begin end
  case (P)
    default: my_label : begin end
  endcase
endmodule
