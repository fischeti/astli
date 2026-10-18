// A variable an always_comb, always_ff, continuous assignment or instance
// drives may have no other driver; two plain processes may share one.
module sub (output logic o);
  assign o = 1'b0;
endmodule

module m (input logic clk_i, output logic q_o);
  logic comb, ff, cont, inst, plain;
  always_comb comb = 1'b0;
  always_comb comb = 1'b1;
  always_ff @(posedge clk_i) ff <= 1'b0;
  initial ff = 1'b1;
  assign cont = 1'b0;
  always @* cont = 1'b1;
  sub u_sub (.o(inst));
  assign inst = 1'b1;
  always @(posedge clk_i) plain = 1'b0;
  initial plain = 1'b1;
  assign q_o = 1'b0;
  always_comb q_o = 1'b1;
endmodule

// A driver in a generate arm clashes with one outside it, which exists
// whenever it does.
module nested #(parameter bit Fast = 1'b1) ();
  logic out;
  assign out = 1'b0;
  if (Fast) begin : gen_fast
    assign out = 1'b1;
  end
endmodule
