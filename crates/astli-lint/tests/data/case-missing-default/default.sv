// A `case` needs a `default`, unless it is `unique` or `unique0`, whose
// qualifier says no other value occurs. `priority` says no such thing. A
// `default` in any branch of an `ifdef` counts, and a `randcase` has no
// `default` to give.
module default_items (
  input  logic [1:0] s_i,
  output logic       x_o
);
  always_comb begin
    case (s_i)
      2'd0: x_o = 1'b0;
      2'd1: x_o = 1'b1;
    endcase
    case (s_i)
      2'd0:    x_o = 1'b0;
      default: x_o = 1'b1;
    endcase
    unique case (s_i)
      2'd0: x_o = 1'b0;
    endcase
    unique0 case (s_i)
      2'd0: x_o = 1'b0;
    endcase
    priority casez (s_i)
      2'b1?: x_o = 1'b0;
    endcase
    case (s_i)
      2'd0: x_o = 1'b0;
`ifdef SIM
      default: x_o = 1'bx;
`endif
    endcase
  end

  initial randcase
    1: x_o = 1'b0;
    3: x_o = 1'b1;
  endcase
endmodule
