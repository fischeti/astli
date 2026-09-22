// Multiplication binds tighter than addition, the ordinary operators are
// left-associative, and `**` is right-associative.
assign x = a + b * c;
assign x = a * b + c;
assign x = a - b - c;
assign x = a / b / c;
assign x = a ** b ** c;
// 11.3.2 puts the unary operators above `**`, so this is `(-2) ** 2`, not
// `-(2 ** 2)` as in most languages.
assign x = -2 ** 2;
