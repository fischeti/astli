// Statements, and the scopes blocks and loops declare their variables in.
module m;
  always_ff @(posedge clk_i or negedge rst_ni) begin : seq
    if (!rst_ni) q <= '0;
    else q <= d;
  end
  always_comb begin
    int k;
    y = 0;
    for (int i = 0, j = 1; i < 4; i++, j--) y += i;
    foreach (arr[a, b]) arr[a][b] = 0;
    unique case (sel)
      2'b00, 2'b01: y = 1;
      default: ;
    endcase
    while (k < 3) k++;
    do k--; while (k > 0);
    repeat (2) k = k + 1;
  end
  always @* y2 = |x;
  always @(a iff en, posedge b) z = a;
  initial fork : f
    #1 ev_next = 1;
    -> ev;
    @(ev);
    wait (done) disable f;
    wait fork;
    step: x = #2 y;
  join_none
  initial begin
    forever begin
      q <= repeat (2) @(posedge clk) d;
      break;
    end
    force a = 1;
    release a;
    assert (a) else $error("a is %0d", a);
  end
endmodule
