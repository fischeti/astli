// Every if, else, loop and procedural block takes `begin`. `else if` is one
// construct, and a timing control stands between an always and its body. A
// macro call may be the block.
module bodies (
  input  logic clk_i,
  input  logic a_i,
  output logic q_o
);
  always_ff @(posedge clk_i) q_o <= a_i;
  always_ff @(posedge clk_i) begin
    if (a_i) q_o <= 1'b0;
    else if (!a_i) begin
      q_o <= 1'b1;
    end else q_o <= 1'b0;
  end
  always_comb begin
    for (int i = 0; i < 2; i++) q_o = a_i;
    foreach (arr[i]) q_o = arr[i];
    while (a_i) q_o = 1'b0;
  end
  initial forever #5 q_o = ~q_o;
  initial `DO_THINGS
  for (genvar g = 0; g < 2; g++) begin : gen_g
  end
endmodule
