// A package member is found from its declaration and from an import,
// with each scoped name of it, and the package from its declaration. An
// enum member, a typedef and a function are found as any other name.
package packages_pkg;
//      ^
  typedef enum logic { Off, On } mode_t;
//                          ^    ^
  function automatic mode_t flip(mode_t m);
//                          ^
    return m == On ? Off : On;
  endfunction
endpackage

module packages;
  import packages_pkg::flip;
//                     ^
  packages_pkg::mode_t mode;
  assign mode = flip(packages_pkg::On);
endmodule

module packages_star;
  import packages_pkg::*;
  mode_t mode = Off;
  assign mode = flip(mode);
endmodule
