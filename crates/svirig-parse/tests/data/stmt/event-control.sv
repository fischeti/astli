initial begin
  // A sensitivity list is read rather than skipped.
  @(posedge clk or negedge rst_n) q <= d;
  @* x = 1;
  @(*) x = 1;
  @ev x = 1;
  // A timing control may be the whole statement.
  @(posedge clk);
  #10;
end
