// The `,` ends the element. A declarator loop that took it would find the
// next element's keyword where a name should be.
module m #(parameter int A = 1, localparam int B = 2) ();
endmodule
