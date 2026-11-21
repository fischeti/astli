// What typing rejects is rejected wherever a body evaluates it: in the
// action of a concurrent assertion, a `case` label, the object a method is
// called on, a stream, a range, a `with`, a tagged value, and a timing,
// whether a statement's or an assignment's.
module top (input logic c);
  typedef enum logic [1:0] {Idle, Busy} state_e;
  typedef union tagged {int A;} u_t;
  state_e s;
  logic [1:0] v;
  logic [3:0] w;
  real r;
  int n, m, k;
  int q [2][$];
  int d [$];
  u_t u;

  always @(posedge c) assert property (c) else s = v;

  always_comb begin
    case (n)
      r % 2: n = 0;
      default: n = 1;
    endcase
    n = q[r].size();
    w = {<<{r & 1}};
    n = n inside {[r % 2 : 3]};
    n = d.sum() with (item & r);
    u = tagged A (r & 1);
  end

  always @(posedge c iff (r & 1)) m = 0;
  initial k = @(posedge r) 1;
endmodule
