// `.*` is reported; `.port` and `.port(signal)` are by name.
module star;
  sub u_a (.*);
  sub u_b (.clk_i, .d_i(d), .*);
  sub u_c (.clk_i, .d_i(d));
endmodule
