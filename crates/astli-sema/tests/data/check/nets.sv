// A procedure may assign only a variable: a net declared as one, or a port
// with no data type, or an input, is reported wherever an `initial`, an
// `always` or a task assigns it. A `force`, a continuous assignment, a
// clocking block's drive and a variable, an output with a data type or a
// non-ANSI port its body declares as one, are not.
module m #(parameter int W = 2) (
  input logic clk_i,
  input logic in_i,
  output [W-1:0] bare_o,
  output wire net_o,
  output logic var_o,
  output var plain_o,
  inout pad_io
);
  wire w;
  tri [1:0] t;
  logic v;
  initial w = 1'b0;
  always_comb bare_o = '0;
  always_ff @(posedge clk_i) begin
    net_o <= in_i;
    t[0] <= 1'b1;
    {v, w} <= 2'b0;
  end
  always @* var_o = in_i;
  initial begin
    plain_o = 1'b0;
    in_i = 1'b0;
    pad_io = 1'b0;
    force w = 1'b1;
    release w;
  end
  task automatic drive();
    w = 1'b1;
  endtask
  assign w = v;
  clocking cb @(posedge clk_i);
    output net_o;
  endclocking
  initial cb.net_o <= 1'b1;
endmodule

module old (q, r);
  output q, r;
  reg q;
  initial q = 1'b0;
  initial r = 1'b0;
endmodule
