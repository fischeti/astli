// Required operands include expressions and macros in raw source.
`define VALUE 1
module m;
    assign a = 1 + 2;
    assign b = `VALUE;
    assign c = `UNRESOLVED;
    initial begin
        a = b;
        a += 2;
        b <= #1 a;
    end
endmodule
