// Package macros must not change dispatch for ordinary imports or DPI declarations.
`define OTHER_PKG other
`define PKG(name) name
package p;
    import `OTHER_PKG::*;
    import `OTHER_PKG::item;
    import `PKG(other)::*;
    export `OTHER_PKG::*;
    import other::*;
    import \other.pkg ::item;
    import "DPI-C" function void c_fn(input int value);
    export "DPI-C" function c_fn;
endpackage
