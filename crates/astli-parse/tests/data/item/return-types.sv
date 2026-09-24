// A return type is told from a name by what follows the `::` chain.
class C;
  function pkg::t f();
  endfunction
endclass
function void C::f();
endfunction
function f();
endfunction
