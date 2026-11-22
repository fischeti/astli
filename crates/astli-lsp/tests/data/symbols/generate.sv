// Generate blocks nest as written; one without a label is not a symbol,
// and what it declares is listed in the block around it.
module gen #(parameter int N = 4) ();
  genvar i;
  for (i = 0; i < N; i++) begin : g_outer
    logic x;
    for (genvar j = 0; j < 2; j++) begin : g_inner
      logic y;
      if (j == 0) begin : g_first
        logic z;
      end else begin
        logic w;
      end
    end
  end

  case (N)
    1: begin : g_one
      wire one;
    end
    default: ;
  endcase

  generate
    if (N > 2) begin : g_big
      wire big;
    end
  endgenerate

  // Processes are named by the label of the block they run, or by their
  // keyword.
  always_ff @(posedge clk or negedge rst_n) begin : p_count
    logic [3:0] next;
    if (!rst_n) begin
      count <= '0;
    end else begin
      count <= count + 1;
    end
  end
  always_comb begin
    out = in;
  end
  initial begin
    fork : f_wait
      #1;
      #2;
    join
  end
  assign y = x;
endmodule
