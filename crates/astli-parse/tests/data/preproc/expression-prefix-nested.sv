// Nested alternatives and an empty branch share the final operand.
module m;
    bit a, b, c, disabled;
    initial begin
        if (
`ifdef EXTRA
`ifdef DEEP
            a && // The RHS follows both conditionals.
`else
            b ||
`endif
`elsif OTHER
            c &&
`else
            // No prefix in this configuration.
`endif
            !disabled) begin
        end
    end
endmodule
