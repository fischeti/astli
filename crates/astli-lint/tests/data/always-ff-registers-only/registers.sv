// Registers alone: a reset, asynchronous or not, an enable, and values that
// only connect bits. A select's index is not judged, so a register file's
// read and write pass, and a loop over an array of registers is repetition.
// A macro may expand to anything, so a call to one passes.
module registers #(
  parameter logic [7:0] RstVal = 8'h5a
) (
  input  logic              clk_i,
  input  logic              rst_ni,
  input  logic              en_i,
  input  cfg_pkg::cfg_t     cfg_i,
  input  logic        [3:0] waddr_i,
  input  logic        [3:0] raddr_i,
  input  logic        [7:0] wdata_i,
  input  logic signed [7:0] d_i,
  output logic        [7:0] rdata_o
);
  logic          [ 7:0] data_q, data_d, count_q;
  logic          [15:0] wide_q;
  logic          [ 7:0] mem_q   [16];
  logic          [ 3:0] en_q;
  logic    [3:0] [ 7:0] lanes_q, lanes_d;
  cfg_pkg::state_e      state_q, state_d;
  cfg_pkg::cfg_t        cfg_q;

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      data_q  <= '0;
      count_q <= RstVal;
      state_q <= cfg_pkg::Idle;
      cfg_q   <= '{default: '0};
      wide_q  <= 16'(0);
    end else if (en_i) begin
      data_q  <= data_d;
      count_q <= -1;
      state_q <= state_d;
      cfg_q   <= cfg_i;
      wide_q  <= {data_d, count_q[7:0]};
    end
  end

  always_ff @(posedge clk_i) begin
    if (~rst_ni) begin
      rdata_o <= '0;
    end else if (cfg_i.en) begin
      rdata_o <= mem_q[raddr_i];
    end
    if (en_q[0]) mem_q[waddr_i] <= wdata_i;
  end

  always_ff @(posedge clk_i) begin
    for (int i = 0; i < 4; i++) begin
      lanes_q[i] <= lanes_d[i];
    end
    wide_q <= $signed(d_i);
    data_q <= {2{d_i[3:0]}};
    data_q <= `RESET_VALUE;
  end
endmodule
