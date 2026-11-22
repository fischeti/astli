// A header's branches and calls are its own file's, each time it is
// included.
`define FIRST
`include "included.svh"
`undef FIRST
`include "included.svh"
