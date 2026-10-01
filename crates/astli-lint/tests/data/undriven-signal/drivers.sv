// Read and never written, or an output never driven; written by any
// assignment, initialiser, output connection, loop or call is driven.
module sub (output logic o);
  assign o = 1'b1;
endmodule

module m (input logic clk_i, output logic q_o, output logic spare_o);
  logic never, assigned, from_init = 1'b0, from_instance, from_task;
  logic [3:0] mem [4];
  supply1 vdd;
  int k;
  assign assigned = 1'b1;
  sub u_sub (.o(from_instance));
  task automatic t(output logic x);
    x = 1'b0;
  endtask
  initial t(from_task);
  always_ff @(posedge clk_i) begin
    foreach (mem[i]) k = i;
    q_o <= never & assigned & from_init & from_instance & from_task & vdd & mem[0][0];
  end
endmodule
