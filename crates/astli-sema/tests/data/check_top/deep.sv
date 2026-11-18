// A definition that instantiates itself goes as deep as its parameters
// say, each level an instance and a generate block; what stands 80 levels
// down is checked, as the tree shows it.
module chain #(parameter int N = 0) ();
  if (N < 40) begin : gen_next
    chain #(.N(N + 1)) u_next ();
  end else begin : gen_end
    logic x;
    assign x = 1'b0;
    assign x = 1'b1;
  end
endmodule

module top;
  chain u_chain ();
endmodule
