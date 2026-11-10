// One `type` keyword declares every type parameter after it in the list,
// each with its own default, which may be a type declared in place.
module m #(type T = int, U = bit, V = enum {A}) ();
endmodule
