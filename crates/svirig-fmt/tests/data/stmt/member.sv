// A field or a scope member takes no space around its `.` or `::`, and
// nothing breaks there.
module m;
  assign a = req . valid;
  assign b = cfg.ral.ctrl .en;
  assign c = pkg :: Width;
  initial obj . method(x);
  initial pkg::C::create(y);
endmodule
