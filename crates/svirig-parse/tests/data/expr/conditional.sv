// `?:` nests to the right, is looser than any binary operator, and is
// tighter than implication.
assign x = a ? b : c ? d : e;
assign x = a || b ? c + 1 : d;
assign x = a -> b ? c : d;
