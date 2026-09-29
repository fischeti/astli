// A label written twice, in one item or two, whatever the spacing, is
// reported where it is never matched. Labels in different branches of one
// `ifdef` are not both built. Labels are compared as written, so `2'd1` and
// `'d1` pass, and a `randcase`'s weights may repeat. A nested `case` has its
// own labels.
module labels (
  input  state_e     s_i,
  input  logic [1:0] n_i,
  output logic       x_o
);
  always_comb begin
    unique case (s_i)
      Idle, Busy: x_o = 1'b0;
      Done, Idle: x_o = 1'b1;
      Busy:       x_o = 1'b1;
`ifdef FAST
      Wait:       x_o = 1'b0;
`else
      Wait:       x_o = 1'b1;
`endif
      default: begin
        case (n_i)
          2'd1:   x_o = 1'b0;
          'd1:    x_o = 1'b0;
          2 'd 1: x_o = 1'b1;
          default: ;
        endcase
      end
    endcase
  end

  initial randcase
    1: x_o = 1'b0;
    1: x_o = 1'b1;
  endcase
endmodule
