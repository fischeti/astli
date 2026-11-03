// A packed struct's member takes no default, as an unpacked one's may. A
// positional pattern has one item for each element of an unpacked array,
// or each member of a struct, nested patterns too; one with keys, a typed
// one and a replication are not counted. A stream unpacks only into what is
// as wide. A `specparam` stands only in a specify block or another one.
module m;
  parameter c = 4'h5;
  typedef struct packed {
    bit [3:0] lo = c;
    bit [3:0] hi;
  } packed_t;
  typedef struct {
    int a = 1;
    int b;
  } pair_t;
  pair_t flat [1:0] = '{0, 0, 1, 1};
  pair_t nested [1:0] = '{'{0, 0}, '{1, 1}};
  int short_by_one [3] = '{1, 2};
  int filled [3] = '{default: 0};
  int repeated [3] = '{3{1}};
  pair_t too_many = '{1, 2, 3};
  int a = 1, b = 2;
  specparam delay = 50;
  specparam twice = delay * 2;
  parameter p = delay + 2;
  initial begin
    static int wide = {<<{a, b, a}};
    static int fits = {<<{a}};
  end
endmodule
