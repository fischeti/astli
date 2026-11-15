// An enum's methods are constant where their value is: `num`, `first` and
// `last` ask only the type, `next`, `prev` and `name` the value too. `next`
// and `prev` wrap around the members, and from a value no member has give
// the type's initial value, as `name` gives the empty string. A method
// without arguments may leave out its parentheses. An array sized by `num`
// holds what its pattern gives it.
module m;
  typedef enum logic [1:0] {A, B = 2, C} abc_e;
  typedef enum bit [1:0] {X, Y} xy_e;
  parameter abc_e P = B;
  localparam int Num = P.num();
  localparam int NumBare = P.num;
  localparam abc_e First = P.first();
  localparam abc_e Last = P.last;
  localparam abc_e Next = P.next();
  localparam abc_e NextTwo = P.next(2);
  localparam abc_e NextWrap = Last.next;
  localparam abc_e Prev = P.prev();
  localparam abc_e PrevWrap = First.prev(1);
  localparam abc_e PrevMany = P.prev(5);
  localparam abc_e NotMember = abc_e'(1);
  localparam abc_e NextNone = NotMember.next();
  localparam xy_e NextNoneTwoState = xy_e'(3).next();
  localparam string Name = P.name();
  localparam string NameBare = Last.name;
  localparam string NameNone = NotMember.name();
  localparam int Size = P.last() + 1;
  localparam int unsigned Ns[P.num()] = '{1, 2, 3};
  localparam int unsigned N = Ns[P.num() - 1];
endmodule
