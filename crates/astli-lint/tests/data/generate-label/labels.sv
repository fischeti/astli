// The body of a generate `if`, `for` or `case` item has a label. A block in
// procedural code, a function or an assertion's action is not a generate
// block.
module labels #(parameter bit P = 1) (input logic clk_i);
  if (P) begin : gen_a
  end else begin
  end
  for (genvar i = 0; i < 2; i++) begin
  end
  case (P)
    1'b1: begin : gen_c end
    default: begin : gen_d end
  endcase
  always_comb begin
    if (P) begin end
  end
  function automatic void f();
    for (int i = 0; i < 2; i++) begin end
  endfunction
  assert property (@(posedge clk_i) P) else begin $error("P"); end
endmodule
