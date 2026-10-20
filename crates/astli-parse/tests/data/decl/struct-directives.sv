// A directive or a conditional region stands among a struct's members, and
// a macro call may stand for a member's type.
typedef struct packed {
  logic a;
`ifdef X
  logic b;
`endif
  `MY_T c;
} s_t;
