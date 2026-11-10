// One `type` keyword declares every type parameter after it in the list,
// each with its own default, which may be a type declared in place; and
// SystemVerilog 2023 lets the keyword restrict what type may be given.
module m #(type T = int, U = bit, V = enum {A}) ();
endmodule

module n #(parameter type enum E = enum {B}, type struct S = struct {int a;}, type union W) ();
endmodule
