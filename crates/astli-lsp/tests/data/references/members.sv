// A struct's member is found from its declaration and from a use, through
// a typedef and through a type parameter, whose default names the typedef
// or declares the struct in place.
typedef struct packed {
  logic [3:0] low;
//            ^
  logic high;
} members_pair_t;

module members #(parameter type pair_t = members_pair_t) ();
  members_pair_t pair;
  pair_t other;
  logic [3:0] got;
  assign got = pair.low;
//                  ^
  assign got = other.low;
endmodule

module members_inline #(
  localparam type par_t = struct packed { logic x; logic y; }
//                                              ^
) ();
  par_t b;
  logic o;
  assign o = b.x;
endmodule
