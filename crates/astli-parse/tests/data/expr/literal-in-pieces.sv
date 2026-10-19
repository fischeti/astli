// 5.7.1 lets whitespace separate a size from its base and a base from its
// digits. Each of these is one LITERAL_EXPR, which is what stops a formatter
// coming between the pieces.
assign x = 8'hFF;
assign x = 8 'h FF;
assign x = 'h FF;
assign x = '0;
assign x = 8 'h FF + 1;
// `4a43_f880` lexes as an integer and then an identifier. Nothing separates
// them, which is what says they are one value.
assign x = 256'h 4a43_f880;
// `?` is a `z` digit, and apart from its base it is a token of its own, here
// and in the pieces after it; the `?` after the condition is not a digit.
assign x = 2'b ?1;
assign x = 4'b 1?0?;
assign x = c ? 4'b 10 : 4'b 01;
