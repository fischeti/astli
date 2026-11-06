// A missing RHS needs a diagnostic, and the next assignment must still parse.
module m;
    assign a = ;
    assign b = 1;
endmodule

module recovery;
    initial begin
        a = /* an operand is still required */ ;
        b = ;
        c = 2;
        d = (e = );
    end
endmodule
