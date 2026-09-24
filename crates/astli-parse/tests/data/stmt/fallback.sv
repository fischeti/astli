initial begin
  assert property (@(posedge clk) a |-> b);
  randcase 1 : x = 1; endcase
end
