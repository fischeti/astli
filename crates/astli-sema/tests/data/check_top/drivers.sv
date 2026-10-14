// A variable two things drive in what is built: each generate block built,
// each loop iteration too, is a place of its own; arms not built drive
// nothing; writes to different parts of a variable do not clash.
module sub (output logic q);
  assign q = 1'b0;
endmodule

module top #(parameter int N = 2) (input logic clk, input logic d);
  logic clashes, iterated, parts, exclusive, local_ok;
  logic [N-1:0] bits;

  always_ff @(posedge clk) clashes <= d;
  assign clashes = d;

  for (genvar i = 0; i < N; i++) begin : g_whole
    assign iterated = d;
  end

  for (genvar i = 0; i < N; i++) begin : g_part
    assign bits[i] = d;
  end

  if (N > 1) begin : g_one
    assign exclusive = d;
  end else begin : g_other
    assign exclusive = ~d;
  end

  for (genvar i = 0; i < N; i++) begin : g_local
    logic mine;
    assign mine = d;
  end

  sub u_a (.q(parts));
  sub u_b (.q(parts));
endmodule
