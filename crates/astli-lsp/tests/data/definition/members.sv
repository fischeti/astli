// A struct's member used goes to its declaration in the struct, which goes
// to itself.
typedef struct packed {
  logic [3:0] low;
//            ^
  logic high;
} pair_t;

module members;
  pair_t pair;
  logic [3:0] got;
  assign got = pair.low;
//                  ^
endmodule
