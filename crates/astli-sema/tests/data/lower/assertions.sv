// A concurrent assertion among items runs as a process of its own, labelled
// or not, and in a procedural block is a statement; so does a deferred
// immediate one. A property and a
// sequence declare their formals and local variables in a scope of their
// own, and a formal may default to a sequence. Each operator keeps its
// operands: a delay and a repetition their bounds, a match what it assigns.
module m;
  logic clk, rst, a, b, c, d;
  default disable iff (rst);
  sequence s_req (x, int n = 2, sequence t = x ##1 x);
    int cnt;
    (x, cnt = 0) ##1 (t, cnt++) [*n] ##[*] a ##[+] b;
  endsequence
  property p_handshake (req, ack);
    @(posedge clk) disable iff (rst)
    req |-> if (ack) s_eventually [1:$] c else always d;
  endproperty
  a_handshake: assert property (p_handshake(a, b)) else $error("lost");
  cover property (@(posedge clk) a [->1:3] ##1 b [=2] within s_req(c));
  restrict property (@(posedge clk) not a and accept_on (b) c |-> @(negedge clk) d);
  a_same: assert final (a == b) else $fatal(1, "differ");
  assume #0 (c || d);
  always @(posedge clk) begin
    assume property (a |=> ##[1:3] b);
    expect (@(posedge clk) a ##1 b) else $error("e");
  end
endmodule
