// option: clear false
// A project that forbids a clear lets a flop take one reset: asynchronous,
// or, in a process that waits for its clock alone, synchronous.
module clears (
  input  logic       clk_i,
  input  logic       rst_ni,
  input  logic       clear_i,
  input  logic       load_i,
  input  logic [7:0] data_i
);
  logic [7:0] a_q, b_q, c_q, d_q;

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) a_q <= '0;
    else if (load_i) a_q <= data_i;
  end

  always_ff @(posedge clk_i) begin
    if (!rst_ni) b_q <= '0;
    else if (load_i) b_q <= data_i;
    c_q <= !rst_ni ? '0 : data_i;
  end

  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) begin
      d_q <= '0;
    end else begin
      if (clear_i) begin
        d_q <= '0;
      end else if (load_i) begin
        d_q <= data_i;
      end
    end
  end
endmodule
