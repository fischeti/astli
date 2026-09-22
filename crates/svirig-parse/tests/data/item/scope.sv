// A subroutine body holds statements and a module body holds items, so
// `x = 1;` is a statement in the first and a fallback run in the second.
module m;
  function int f();
    x = 1;
  endfunction
  x = 1;
endmodule
