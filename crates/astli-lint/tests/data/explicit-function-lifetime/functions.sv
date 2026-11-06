// A function outside a class says `automatic` or `static`, in a module or
// a package alike; a class's methods are automatic already, and a DPI
// import or export takes no lifetime.
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
  import "DPI-C" function int f(input int a);
  export "DPI-C" function twice;
endpackage
