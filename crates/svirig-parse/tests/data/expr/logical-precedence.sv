// The logical and bitwise operators climb in the order Table 11-2 gives.
assign x = a || b && c;
assign x = a && b | c;
assign x = a | b ^ c;
assign x = a ^ b & c;
assign x = a & b == c;
assign x = a == b < c;
assign x = a < b << c;
