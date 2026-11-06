// A missing RHS needs a diagnostic, and the next assignment must still parse.
module m;
    assign a = ;
    assign b = 1;
endmodule
