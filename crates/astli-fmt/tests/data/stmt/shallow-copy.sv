// A shallow copy is spaced like any assignment.
module m;
  initial begin
    copy=new cfg.dut_cfg;
    other = new   original;
  end
endmodule
