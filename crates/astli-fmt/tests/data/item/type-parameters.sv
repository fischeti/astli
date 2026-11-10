// A type parameter that leaves out `type`, continuing the one before it, is
// a row of its own, as a value parameter that leaves out its type is.
module m #(parameter type T = int, U = bit, parameter int W = 1) ();
endmodule
