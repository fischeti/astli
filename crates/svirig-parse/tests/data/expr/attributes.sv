// 11.3.2 admits an attribute between an operator and its operand, the one
// place inside an expression. `*` is not a prefix operator, so `(` followed
// by one can only open an attribute.
assign x = a + (* full_case *) b;
assign x = a + (* x = 1 *) b;
// And a parenthesised expression is not one.
assign x = (a);
assign x = (a * b);
