// An assignment without a right-hand side falls back, and what follows parses.
module m;
    assign a = ;
    assign b = 1, c = ;
    assign d = 1;
    initial begin
        e = /* an operand is still required */ ;
        f <= #1 ;
        g = 2;
        for (int i = 0; i < 4; i += ) h = 3;
        assign j = ;
        k = 4;
    end
endmodule
