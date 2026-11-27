// What a reset or an enable is, told by what its branches load: a
// condition that loads only constants on one side, on no further
// condition, is a reset or a clear, a reset and a clear before an enable
// at most; one that loads data on one side and
// nothing, or the register's own value, on the other is an enable. Data on
// both sides is a mux, a condition under an enable is a condition of two
// signals, and a register two statements load is a mux the last one wins,
// whether written as `if` or as `?:`. Registers apart, or members or
// constant indices apart, may be loaded by statements apart. An
// asynchronous reset is a signal the process waits for, whatever it loads,
// and a constant condition chooses nothing.
module structure #(
  parameter bit         Wide   = 1'b1,
  parameter logic [7:0] Rst[2] = '{8'h1, 8'h2}
) (
  input  logic       clk_i,
  input  logic       rst_ni,
  input  logic       start_i,
  input  logic       done_i,
  input  logic       err_i,
  input  logic       clear_i,
  input  logic       load_i,
  input  logic [7:0] data_i,
  input  logic [7:0] other_i,
  input  logic [7:0] boot_i
);
  typedef struct packed {
    logic [7:0] a;
    logic [7:0] b;
  } pair_t;

  logic       busy_q;
  logic [7:0] a_q, b_q, c_q, d_q, e_q, f_q, g_q;
  logic [7:0] pipe_q[2][2];
  logic [7:0] rst_q[2];
  pair_t      pair_q;

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      a_q <= '0;
    end else begin
      if (clear_i) begin
        a_q <= '0;
      end else if (load_i) begin
        a_q <= data_i;
      end
    end
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) b_q <= '0;
    else if (clear_i) b_q <= '1;
    else b_q <= data_i;
  end

  always_ff @(posedge clk_i) begin
    if (load_i) c_q <= data_i;
    else c_q <= '0;
    if (load_i) d_q <= data_i;
    else d_q <= d_q;
    e_q <= !rst_ni ? '0 : load_i ? data_i : e_q;
    if (start_i) pair_q.a <= data_i;
    if (done_i) pair_q.b <= data_i;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (Wide) begin
      if (!rst_ni) begin
        f_q <= boot_i;
      end else begin
        for (int i = 0; i < 2; i++) begin
          pipe_q[i][1] <= pipe_q[i][0];
          pipe_q[i][0] <= data_i;
        end
      end
    end
  end

  for (genvar i = 0; i < 2; i++) begin : gen_rst
    always_ff @(posedge clk_i) begin
      if (clear_i) rst_q[i] <= Rst[i];
      else if (load_i) rst_q[i] <= data_i;
    end
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      busy_q <= 1'b0;
      f_q    <= '0;
    end else begin
      if (start_i) begin
        busy_q <= 1'b1;
        if (err_i) begin
          f_q <= '0;
        end else begin
          f_q <= data_i;
        end
      end
      if (done_i) begin
        busy_q <= 1'b0;
      end
    end
  end

  always_ff @(posedge clk_i) begin
    if (start_i) a_q <= data_i;
    else a_q <= other_i;
    b_q <= start_i ? data_i : other_i;
    if (start_i) begin
      if (load_i) c_q <= data_i;
    end
    d_q <= start_i ? (load_i ? data_i : d_q) : d_q;
    if (start_i) e_q <= data_i;
    if (done_i) e_q <= other_i;
    g_q <= '0;
    if (load_i) g_q <= data_i;
    if (start_i) pair_q.a <= data_i;
    if (done_i) pair_q <= '0;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) a_q <= '0;
    else if (clear_i) a_q <= '0;
    else if (err_i) a_q <= '1;
    else if (load_i) a_q <= data_i;
  end

  always_ff @(posedge clk_i) begin
    if (load_i) b_q <= data_i;
    else if (clear_i) b_q <= '0;
    c_q <= load_i ? data_i : clear_i ? '0 : c_q;
  end
endmodule
