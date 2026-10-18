// Not reported: parts written apart, a net driven twice, drivers in two
// arms of one generate `if` or `case`, a `force` that overrides, writes in
// a function, and one driver writing in many places.
module m #(parameter bit Fast = 1'b0, parameter int Mode = 0) (input logic clk_i);
  logic [1:0] bits;
  wire shared;
  logic arm, picked, forced, in_function, once;
  assign bits[0] = 1'b0;
  assign bits[1] = 1'b1;
  assign shared = 1'b0;
  assign shared = 1'b1;
  if (Fast) begin : gen_fast
    assign arm = 1'b0;
  end else begin : gen_slow
    always_ff @(posedge clk_i) arm <= 1'b1;
  end
  case (Mode)
    0: assign picked = 1'b0;
    default: assign picked = 1'b1;
  endcase
  always_comb forced = 1'b0;
  initial force forced = 1'b1;
  function automatic void set();
    in_function = 1'b1;
  endfunction
  always_comb in_function = 1'b0;
  always_ff @(posedge clk_i) begin
    once <= 1'b0;
    if (arm) once <= 1'b1;
  end
endmodule

// Two generate constructs whose conditions exclude each other, which only
// evaluating them tells.
module complementary #(parameter int Depth = 2) ();
  logic out;
  if (Depth == 2) begin : gen_two
    assign out = 1'b0;
  end
  if (Depth > 2) begin : gen_more
    assign out = 1'b1;
  end
endmodule
