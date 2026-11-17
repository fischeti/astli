// `always_ff` holds registers alone: a reset and an enable, each one signal
// under one `!` or `~`, a mux on one signal written either as `if` or `?:`,
// and values that connect bits or are constants. A select's index, a loop's
// header and an assertion are not judged, nor is `always_comb`. A flop a
// macro writes is judged as a written one, and reported at the call.
`define FF(q, d, rv) always_ff @(posedge clk_i or negedge rst_ni) if (!rst_ni) q <= (rv); else q <= (d);
`define FFSRN(q, d, rv) always_ff @(posedge clk_i) q <= (!rst_ni) ? (rv) : (d);
`define INC(q) q <= q + 1;

module registers #(
  parameter int unsigned Width = 8
) (
  input  logic             clk_i,
  input  logic             rst_ni,
  input  logic             en_i,
  input  logic             stall_i,
  input  logic       [3:0] addr_i,
  input  logic [Width-1:0] d_i,
  output logic [Width-1:0] q_o
);
  logic [Width-1:0] a_q, cnt_q, mem_q[16];

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      a_q <= Width'(Width - 1);
      q_o <= '{default: '0};
    end else if (en_i) begin
      a_q <= {d_i[3:0], a_q[Width-1:4]};
      q_o <= mem_q[addr_i + 1];
    end
    for (int i = 0; i < 16; i++) if (~stall_i) mem_q[i] <= d_i;
    q_o <= en_i ? d_i : stall_i ? '0 : q_o;
    assert (cnt_q != '1);
  end

  `FF(cnt_q, d_i, '0)
  `FFSRN(cnt_q, d_i, '0)
  `FF(cnt_q, cnt_q + 1, '0)

  always_ff @(posedge clk_i) begin
    `INC(cnt_q)
    if (en_i && !stall_i) q_o <= d_i;
    if (en_i == 1'b1) q_o <= d_i;
    if (!(en_i || stall_i)) q_o <= ~d_i;
    case (addr_i)
      4'd0:    q_o <= d_i;
      default: q_o <= d_i;
    endcase
    q_o <= (cnt_q == 2) ? d_i : q_o;
    q_o <= en_i ? d_i + 1 : q_o;
    cnt_q += 1;
    cnt_q++;
    q_o <= {d_i[3:0], d_i[7:4] ^ 4'hf};
    q_o <= $signed(d_i);
    $display("%d", cnt_q);
  end

  always_ff @(posedge clk_i) begin
    automatic logic [Width-1:0] sum = d_i + 1;
    q_o <= sum;
  end

  always_comb q_o = en_i ? d_i + 1 : '0;
endmodule
