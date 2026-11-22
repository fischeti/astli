// A built-in has no `define, and a call and name of its own token. The
// `line keeps `__FILE__ to a name that does not depend on the checkout.
`line 3 "builtins.sv" 0
initial $display(`__FILE__, `__LINE__);
