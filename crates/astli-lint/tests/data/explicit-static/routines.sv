// A routine's variables take its lifetime: static if it says so, or if it
// says nothing outside a class and an automatic design element, and
// automatic otherwise. A method is automatic, in its class or out of it.
package routines;
  function int once();
    int n = 0;
    return n;
  endfunction
  function static int counter();
    int n = 0;
    return n;
  endfunction
  function automatic int fresh();
    int n = 0;
    return n;
  endfunction
  task waits();
    begin
      int n = 0;
    end
  endtask
  class item;
    static function int size();
      int n = 0;
      return n;
    endfunction
    extern function int total();
  endclass
  function int item::total();
    int n = 0;
    return n;
  endfunction
endpackage

package automatic fresh_pkg;
  function int fresh();
    int n = 0;
    return n;
  endfunction
endpackage
