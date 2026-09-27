// Among statements, `assign` is procedural, as `force` is.
module m;
  initial begin
    force tb.dut.a = 1'b0;
    release tb.dut.a;
    assign q = d;
    deassign q;
  end
endmodule
