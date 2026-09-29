// A function outside a class says `automatic` or `static`, in a module or
// a package alike; a class's methods are automatic already.
package functions;
  function automatic int twice(int a);
    return 2 * a;
  endfunction
  function int thrice(int a);
    return 3 * a;
  endfunction
  function static int count();
    return 0;
  endfunction
  class item;
    function int size();
      return 0;
    endfunction
  endclass
endpackage
