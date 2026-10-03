// An import item is one package and one name or `*`: packages do not nest,
// so `p::q::A` is kept as written, as is an item with no name.
module m;
  import p::A, q::*;
  export *::*;
  import p::q::A;
  import p::;
endmodule
