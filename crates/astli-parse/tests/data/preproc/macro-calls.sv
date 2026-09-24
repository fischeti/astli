`MY_MACRO
`define A(x, y) x
`A(1, 2)
// Arguments split only at depth zero.
`A(f(1, 2), {a, b})
// The list is split on commas and nothing else, so `A() passes one argument
// and `A(,) two.
`A()
`A(,)
