// A reset as lowRISC takes it: asynchronous and active low, the whole of a
// flop's process an `if` on it, through `begin` and labels, tested with
// `!`, `~` or a comparison. Elsewhere a reset is passed to a port whole,
// copied or replicated into another, waited for, read by an assertion, or
// tied off into a signal named unused. A flop without a reset needs none.
module resets (
  input  logic       clk_i,
  input  logic       rst_ni,
  input  logic       en_i,
  input  logic [7:0] d_i,
  output logic [7:0] q_o
);
  logic       [7:0] data_q;
  logic             rst_sync_ni;
  logic       [1:0] rst_vec_ni;
  logic             rst_core_ni = rst_ni;
  logic             unused_rst;
  logic             addr_rst_ni;

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      q_o <= '0;
    end else if (en_i) begin
      q_o <= d_i;
    end
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin : g_flop
    if (~rst_ni) data_q <= '0;
    else data_q <= d_i;
  end

  always_ff @(negedge rst_ni, posedge clk_i) if (rst_ni == 1'b0) data_q <= '0;

  always @(posedge clk_i or negedge rst_ni) if (!rst_ni) data_q <= '0;

  always_ff @(posedge clk_i) data_q <= d_i;

  assign rst_sync_ni = rst_ni;
  assign rst_vec_ni  = {2{rst_ni}};
  assign addr_rst_ni = rst_ni;
  assign unused_rst  = ^{clk_i, rst_ni};

  sub u_sub (
    .clk_i,
    .rst_ni (rst_ni),
    .rsts_ni(rst_vec_ni[1])
  );

  default disable iff (!rst_ni);
  assert property (@(posedge clk_i) disable iff (!rst_ni) en_i |-> d_i != '0);
endmodule

// A bench drives the reset, with constants, and waits for its edge.
module tb;
  logic rst_ni;
  initial begin
    rst_ni = 1'b0;
    #10 rst_ni = 1'b1;
    @(posedge rst_ni);
  end
endmodule
