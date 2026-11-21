// A project's own marks replace the default, matched in any case and at
// the ends they anchor: `x_nc_y` and `unused_x` are reported.
// option: ignore *_nc dummy_*
module m;
  logic a_nc, B_NC, dummy_b, x_nc_y, unused_x;
endmodule
