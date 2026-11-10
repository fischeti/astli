// A subroutine's port may be `const ref`, declared in the header or in the
// body.
module m;
  function automatic void g;
    const ref int b;
  endfunction
endmodule
