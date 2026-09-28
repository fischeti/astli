// A `let` names an expression, with formals if it takes any: typed or
// `untyped`, with defaults. It may stand in a package, a module or a block.
package p;
  let max(a, b) = a > b ? a : b;
endpackage
module m;
  let op(x, y, z) = |((x | y) & z);
  let at_least(logic [3:0] v, untyped lo = 0) = v >= lo;
  let one() = 1;
  let both = a && b;
  initial begin
    let local_sum(q) = q + 1;
    d = op(.x(a), .y(b), .z(c));
  end
endmodule
