// An import takes no space around `::`, and one after each comma.
package p;
  import   a_pkg :: * ;
  import b_pkg::x,c_pkg::y;
  export  a_pkg::*;
endpackage
