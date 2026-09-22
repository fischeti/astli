// `end else begin` share a line, unless the `end` has a label. An `else`
// after any other statement starts a line, and `else if` stays together.
module m;
  always_comb begin
    if (a) begin
      x = 1;
    end
    else if (b)
    begin
      x = 2;
    end else x = 3;
    if (c) y = 1; else y = 2;
    unique if (d) begin : g_d
      z = 1;
    end : g_d else begin
      z = 2;
    end
  end
endmodule
