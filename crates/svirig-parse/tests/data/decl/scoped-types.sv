// A scope settles the shape on its own: only a declaration is written
// `pkg::t x;`.
pkg::t x;
pkg::C #(W) x;
// Unscoped, `C #(W) x;` looks like an instantiation until the port list.
typedef int C;
C #(W) x;
