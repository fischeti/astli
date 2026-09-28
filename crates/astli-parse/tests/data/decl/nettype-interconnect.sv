// A `nettype` names a type for nets, and the function that resolves several
// drivers of one; it may rename another. An `interconnect` is a net whose
// type its connections decide.
module m;
  nettype real real_net;
  nettype real real_sum_net with real_sum;
  nettype pkg::T renamed_net with pkg::resolve;
  interconnect bus;
  interconnect [3:0] a, b[2];
endmodule
