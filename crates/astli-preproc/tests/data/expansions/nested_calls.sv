// The outermost call is listed, and what it wrote holds the inner calls'
// expansions, in its body and in its argument.
`define INNER(x) (x + 1)
`define OUTER(x) `INNER(x) * `INNER(2)
assign y = `OUTER(`INNER(a));
