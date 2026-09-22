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
