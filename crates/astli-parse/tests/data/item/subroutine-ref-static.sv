// A subroutine's port may be `const ref`, declared in the header or in the
// body, and SystemVerilog 2023 adds `ref static`, which a list of ports
// after it shares.
module m;
  function automatic void f(ref static logic a, r);
  endfunction
  function automatic void g;
    const ref int b;
  endfunction
endmodule
