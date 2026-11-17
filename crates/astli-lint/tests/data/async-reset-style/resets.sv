// An asynchronous reset is the whole of its flop's process: an `if` on it
// alone, through `begin`, labels and loops over an array, tested at the
// level its edge leads to with `!`, `~` or a comparison, the events in
// either order. A flop beside that `if` waits for the reset without taking
// it; a process that tests another signal, or none, takes no reset. A flop
// with no reset, or waiting for both edges of its clock, has none to take.
// A flop a macro writes is judged as a written one.
`define FF(q, d) always_ff @(posedge clk_i or negedge (rst_ni)) if (!rst_ni) q <= '0; else q <= (d);
`define FFBAD(q, d) always_ff @(posedge clk_i or negedge rst_ni) if (rst_ni) q <= (d);

module resets (
  input  logic       clk_i,
  input  logic       rst_ni,
  input  logic       rst_i,
  input  logic       rst_aon_ni,
  input  logic       en_i,
  input  logic [7:0] d_i,
  output logic [7:0] q_o,
  output logic [3:0] a_o
);
  always_ff @(posedge clk_i or negedge rst_ni) begin : g_flop
    if (!rst_ni) begin
      q_o <= '0;
    end else if (en_i) begin
      q_o <= d_i;
    end
  end
  always_ff @(negedge rst_ni, posedge clk_i) if (rst_ni == 1'b0) q_o <= '0;
  always_ff @(posedge clk_i or posedge rst_i) if (rst_i) q_o <= '0;
  always_ff @(posedge clk_i or negedge rst_ni)
    for (int i = 0; i < 4; i++) if (~rst_ni) a_o[i] <= 1'b0; else a_o[i] <= d_i[i];
  always_ff @(posedge clk_i) if (!rst_ni) q_o <= '0;
  always_ff @(posedge clk_i or negedge clk_i) q_o <= d_i;
  `FF(q_o, d_i)

  always_ff @(posedge clk_i or negedge rst_ni) if (rst_ni) q_o <= d_i; else q_o <= '0;
  always_ff @(posedge clk_i or posedge rst_i) if (!rst_i) q_o <= '0;
  always_ff @(posedge clk_i or negedge rst_ni) if (rst_ni != 1'b0) q_o <= '0;
  always_ff @(posedge clk_i or negedge rst_ni) begin
    a_o <= d_i[3:0];
    if (!rst_ni) q_o <= '0;
    else q_o <= d_i;
  end
  always_ff @(posedge clk_i or negedge rst_aon_ni) if (!rst_ni) q_o <= '0;
  always_ff @(posedge clk_i or negedge rst_ni) q_o <= d_i;
  `FFBAD(q_o, d_i)
endmodule
