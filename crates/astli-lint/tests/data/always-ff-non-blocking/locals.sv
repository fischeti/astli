// A variable declared inside the block is read by no other process, so a
// blocking assignment to it is fine, as is a loop's own counter. A part of
// one, or a concatenation of them, is local too; one non-local part is not.
module locals (
  input  logic       clk_i,
  input  logic [3:0] d_i,
  output logic [3:0] q_o
);
  always_ff @(posedge clk_i) begin
    automatic logic [3:0] next;
    logic [1:0]           hi, lo;
    next      = d_i;
    next[0]   = 1'b0;
    {hi, lo}  = next;
    {hi, q_o} = next;
    for (int i = 0; i < 4; i++) begin
      q_o[i] <= next[i];
    end
  end
endmodule
