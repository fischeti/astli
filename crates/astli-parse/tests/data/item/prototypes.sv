// A prototype has no body to look for.
class C;
  extern function void f();
  pure virtual function int g();
endclass
package p;
  import "DPI-C" function void h(input int a);
  import "DPI-C" context c_name = function void k();
endpackage
