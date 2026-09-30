// A generate `if` is one member however many `else if`s follow it; a
// named block declares its name where it stands; a loop's genvar belongs to
// its header.
module m;
  if (W > 4) begin : big
    logic z;
  end else if (W > 2) begin
    logic z;
  end else
    assign y = 0;
  case (W)
    0, 1: begin : gen_small logic s; end
    default: ;
  endcase
  for (genvar i = 0; i < 4; i++) begin : g
    assign a[i] = b.c[i];
  end
  for (j = 0; j < 2; j = j + 1) gen_j: begin end
  begin : standalone
    logic w;
  end
  labelled: begin
    logic v;
  end
  generate
    logic in_region;
  endgenerate
endmodule
