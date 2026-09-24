assign x = {a, b};
assign x = {4{a}};
// A stream keeps its direction and its slice size.
assign x = {<<{a}};
assign x = {>>4{a, b}};
// `T'{...}` is one pattern with a type, not a cast applied to a pattern:
// `'{` is a single token, so there is no `'` to cast with.
assign x = req_t'{a: 1};
assign x = '{4{a}};
assign x = '{default: 0};
assign x = '{a, b};
