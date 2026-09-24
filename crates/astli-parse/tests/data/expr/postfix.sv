// A postfix reopens whatever precedes it: selects, calls, scopes and casts
// all chain.
assign x = a.b[3].c(1);
assign x = pkg::T::x;
assign x = a[hi:lo];
assign x = a[base+:width];
assign x = a[base-:width];
assign x = int'(x);
assign x = 8'(y);
assign x = T::U'(z);
assign x = ++a;
assign x = a++;
