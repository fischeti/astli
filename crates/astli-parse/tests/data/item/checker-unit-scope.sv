// A checker instance may name its checker through `$unit::`, as through a
// package.
checker c1;
endchecker

module m;
  $unit::c1 u_c1 ();
endmodule
