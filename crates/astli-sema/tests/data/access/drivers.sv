// What reads and what writes: the left of an assignment is written, a
// compound one also read; a select or member is a part; a connection is as
// its port's direction says; an initialiser writes what it declares.
module sub (input logic i, output logic o, inout wire io);
endmodule

module m (input logic clk_i, input logic [3:0] d_i, output logic [3:0] q_o);
  logic [3:0] q, n;
  logic a, b, c, i;
  wire io;
  wire w = a;
  assign {b, c} = d_i[1:0];
  always_ff @(posedge clk_i) q <= d_i;
  always_comb begin
    n = q;
    n[0] += a;
  end
  assign q_o = n;
  sub u_sub (.i(a), .o(b), .io(w));
  sub u_star (.*, .o(c));
endmodule
