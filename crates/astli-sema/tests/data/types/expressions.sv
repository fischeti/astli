// The type an expression has by itself: an enum kept by the operators
// whose operands share its type, and lost by those that mix it with another;
// a select whatever its index; an untyped pattern in a conditional, from the
// other arm; the methods of an enum, a string and an array; and the system
// functions that return a value.
module m (
  input logic [1:0] i
);
  typedef enum logic [1:0] {Idle, Busy, Done} state_e;

  logic c;
  state_e s, t, u, w, x, y;
  logic [7:0] a, b, d, e;
  logic [7:0] mem [4];
  logic [7:0] row [4];
  int n, k, h;
  string str, low;
  int q [$];
  logic [63:0] now;
  real r;

  assign s = c ? Idle : Busy;
  assign t = u | w;
  assign x = ~u;
  assign a = u + 1;
  assign b = mem[i];
  assign d = mem[i][i];
  assign row = c ? mem : '{default: '0};
  assign y = u.next();
  assign n = u.num();
  assign str = u.name();
  assign k = str.len();
  assign low = str.tolower();
  assign h = q.sum();
  assign e = q.pop_front();
  assign now = $time;
  assign r = $realtime;
endmodule
