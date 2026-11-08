// A small design in the style real IP is written in: a package of types and
// constants, a register file behind a request/response bus, a FIFO, and a
// top that instantiates them with a generate loop.

`define REG_DFF(q, d, clk, rst_n, reset_value) \
  always_ff @(posedge clk or negedge rst_n) begin \
    if (!rst_n) q <= reset_value; \
    else q <= d; \
  end

`ifndef DATA_WIDTH
`define DATA_WIDTH 32
`endif

package bench_pkg;
  localparam int unsigned AddrWidth = 12;
  localparam int unsigned DataWidth = `DATA_WIDTH;
  localparam int unsigned StrbWidth = DataWidth / 8;
  localparam int unsigned NumRegs = 16;

  typedef logic [AddrWidth-1:0] addr_t;
  typedef logic [DataWidth-1:0] data_t;
  typedef logic [StrbWidth-1:0] strb_t;

  typedef enum logic [1:0] {
    Idle,
    Read,
    Write,
    Error
  } state_e;

  typedef struct packed {
    addr_t addr;
    logic  write;
    data_t wdata;
    strb_t strb;
  } req_t;

  typedef struct packed {
    data_t rdata;
    logic  error;
  } rsp_t;

  function automatic data_t apply_strb(data_t old_data, data_t new_data, strb_t strb);
    data_t result;
    for (int unsigned i = 0; i < StrbWidth; i++) begin
      result[i*8+:8] = strb[i] ? new_data[i*8+:8] : old_data[i*8+:8];
    end
    return result;
  endfunction
endpackage

module bench_regfile #(
  parameter int unsigned NumRegs = bench_pkg::NumRegs,
  parameter bit          ResetZero = 1'b1
) (
  input  logic              clk_i,
  input  logic              rst_ni,
  input  logic              req_valid_i,
  output logic              req_ready_o,
  input  bench_pkg::req_t   req_i,
  output logic              rsp_valid_o,
  input  logic              rsp_ready_i,
  output bench_pkg::rsp_t   rsp_o
);
  import bench_pkg::*;

  localparam int unsigned IdxWidth = $clog2(NumRegs);

  data_t [NumRegs-1:0] regs_d, regs_q;
  state_e state_d, state_q;
  rsp_t rsp_d, rsp_q;
  logic [IdxWidth-1:0] idx;
  logic in_range;

  assign idx = req_i.addr[IdxWidth+1:2];
  assign in_range = req_i.addr[AddrWidth-1:IdxWidth+2] == '0;
  assign req_ready_o = state_q == Idle;
  assign rsp_valid_o = state_q != Idle;
  assign rsp_o = rsp_q;

  always_comb begin
    regs_d = regs_q;
    state_d = state_q;
    rsp_d = rsp_q;
    unique case (state_q)
      Idle: begin
        if (req_valid_i) begin
          if (!in_range) begin
            rsp_d = '{rdata: '0, error: 1'b1};
            state_d = Error;
          end else if (req_i.write) begin
            regs_d[idx] = apply_strb(regs_q[idx], req_i.wdata, req_i.strb);
            rsp_d = '{rdata: '0, error: 1'b0};
            state_d = Write;
          end else begin
            rsp_d = '{rdata: regs_q[idx], error: 1'b0};
            state_d = Read;
          end
        end
      end
      Read, Write, Error: begin
        if (rsp_ready_i) state_d = Idle;
      end
      default: state_d = Idle;
    endcase
  end

  `REG_DFF(state_q, state_d, clk_i, rst_ni, Idle)
  `REG_DFF(rsp_q, rsp_d, clk_i, rst_ni, '0)

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      regs_q <= ResetZero ? '0 : '1;
    end else begin
      regs_q <= regs_d;
    end
  end
endmodule

module bench_fifo #(
  parameter int unsigned Depth = 8,
  parameter type         data_t = logic [31:0],
  localparam int unsigned AddrWidth = (Depth > 1) ? $clog2(Depth) : 1
) (
  input  logic  clk_i,
  input  logic  rst_ni,
  input  logic  flush_i,
  output logic  full_o,
  output logic  empty_o,
  output logic [AddrWidth:0] usage_o,
  input  data_t data_i,
  input  logic  push_i,
  output data_t data_o,
  input  logic  pop_i
);
  logic [AddrWidth-1:0] read_ptr_q, write_ptr_q;
  logic [AddrWidth:0] count_q;
  data_t mem_q [Depth];

  assign full_o = count_q == Depth[AddrWidth:0];
  assign empty_o = count_q == '0;
  assign usage_o = count_q;
  assign data_o = mem_q[read_ptr_q];

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      read_ptr_q <= '0;
      write_ptr_q <= '0;
      count_q <= '0;
    end else if (flush_i) begin
      read_ptr_q <= '0;
      write_ptr_q <= '0;
      count_q <= '0;
    end else begin
      if (push_i && !full_o) begin
        write_ptr_q <= (write_ptr_q == AddrWidth'(Depth - 1)) ? '0 : write_ptr_q + 1'b1;
      end
      if (pop_i && !empty_o) begin
        read_ptr_q <= (read_ptr_q == AddrWidth'(Depth - 1)) ? '0 : read_ptr_q + 1'b1;
      end
      case ({push_i && !full_o, pop_i && !empty_o})
        2'b10: count_q <= count_q + 1'b1;
        2'b01: count_q <= count_q - 1'b1;
        default: ;
      endcase
    end
  end

  always_ff @(posedge clk_i) begin
    if (push_i && !full_o) mem_q[write_ptr_q] <= data_i;
  end

`ifndef SYNTHESIS
  initial begin : p_check_depth
    assert (Depth > 0) else $fatal(1, "Depth must be greater than zero");
  end
`endif
endmodule

module bench_top #(
  parameter int unsigned NumPorts = 4
) (
  input  logic                              clk_i,
  input  logic                              rst_ni,
  input  logic            [NumPorts-1:0]    req_valid_i,
  output logic            [NumPorts-1:0]    req_ready_o,
  input  bench_pkg::req_t [NumPorts-1:0]    req_i,
  output logic            [NumPorts-1:0]    rsp_valid_o,
  input  logic            [NumPorts-1:0]    rsp_ready_i,
  output bench_pkg::rsp_t [NumPorts-1:0]    rsp_o
);
  for (genvar i = 0; i < NumPorts; i++) begin : gen_port
    bench_pkg::req_t fifo_req;
    logic fifo_full, fifo_empty;
    logic req_ready;

    bench_fifo #(
      .Depth (4),
      .data_t(bench_pkg::req_t)
    ) i_fifo (
      .clk_i,
      .rst_ni,
      .flush_i(1'b0),
      .full_o (fifo_full),
      .empty_o(fifo_empty),
      .usage_o(),
      .data_i (req_i[i]),
      .push_i (req_valid_i[i]),
      .data_o (fifo_req),
      .pop_i  (!fifo_empty && req_ready)
    );

    assign req_ready_o[i] = !fifo_full;

    bench_regfile #(
      .NumRegs(16)
    ) i_regfile (
      .clk_i,
      .rst_ni,
      .req_valid_i(!fifo_empty),
      .req_ready_o(req_ready),
      .req_i      (fifo_req),
      .rsp_valid_o(rsp_valid_o[i]),
      .rsp_ready_i(rsp_ready_i[i]),
      .rsp_o      (rsp_o[i])
    );
  end
endmodule
