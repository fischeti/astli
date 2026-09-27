// An assertion that does not fit puts `else` on the next line. A property
// keeps the lines it was written on; the declaration around it is laid out.
module m;
  default disable iff (!rst_ni);
  aw_stable: assert property (@(posedge clk_i) (aw_valid && !aw_ready) |=> $stable(aw)) else $error("x");
  assert property (@(posedge clk) disable iff (~rst) a ##1 b[*2] ##[1:3] c |-> ##1 (d, x = y) ##1 e);
  cover property (@(posedge clk) a [->1:3] ##1 b [=2] within c);
  assume property (p_name(a, b));
  restrict property (@(posedge clk) not a and b);
  sequence s_req (x, int n = 2);
int cnt;
    (req, cnt = 0) ##1 (ack, cnt++) [*n];
  endsequence : s_req
  property   p_name (a, b);
    @(posedge clk) disable iff (rst)
    a |-> if (b) s_eventually [1:$] c else always d;
  endproperty
  initial begin
    expect (@(posedge clk) a ##1 b) else $error("e");
  end
endmodule
module n;
  assert property (@(posedge clk)
      a |->
          b) else $error("x");
endmodule
