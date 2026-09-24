// A macro reference may stand for a value, a name or a whole subexpression,
// and raw mode cannot know which, so it is an operand.
assign x = `WIDTH - 1;
assign x = `MAX(a, b) + 1;
