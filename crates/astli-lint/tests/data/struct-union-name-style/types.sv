// A struct or union type is lower_snake_case ending in `_t`, packed or not.
package types;
  typedef struct packed { logic a; } pkt_t;
  typedef struct packed { logic a; } pkt_s;
  typedef union packed { logic a; } Word_t;
  typedef struct { int a; } cfg_t;
  typedef enum logic { A } state_e;
endpackage
