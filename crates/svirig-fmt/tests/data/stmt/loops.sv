// A loop keyword is followed by a space and its header, and the statement it
// repeats follows the rule for any statement. A `for` header has a space after
// each `;` that has a clause after it and none inside its parentheses.
module m;
  for (genvar i = 0;i < N;i++) begin : gen_lanes
    assign y[i] = x[i];
  end
  initial begin
    for(int i = 0, j = 1 ; i < N ; i++ , j += 2) x = 1;
    for ( i = 0 ; ; ) begin end
    for (;;)
      @(posedge clk);
    for (int unsigned i = 0; i < NumRequestsPerChannel; i++) data_q[i] = '0;
    while( ( a ) )begin
      a--;
    end
    repeat (2) @(posedge clk);
    repeat(NumCycles)
      tick();
    forever begin
      @(posedge clk);
    end
    forever #5 clk = ~clk;
  end
endmodule
