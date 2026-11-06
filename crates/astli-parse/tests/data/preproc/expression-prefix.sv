// The optional expression prefix takes its final operand after the conditional.
module m;
    bit enable_coverage, disabled;
    initial begin
        if (
`ifdef EXTRA_CHECK
            enable_coverage &&
`endif
            !disabled) begin
        end
    end
endmodule
