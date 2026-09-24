`define WIDTH 8
`define GUARD
// A body is text: the region in it belongs to wherever the macro is used, and
// closes nothing here.
`define WRAP(x) `ifdef E x `endif
`define A(x) \
  do_thing(x); \
  do_other(x)
