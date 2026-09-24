typedef enum logic [1:0] { A = 0, B } state_e;
typedef struct packed { logic a; int b; } hdr_t;
typedef union { int a; } u_t;
typedef struct { rand mubi4_t en; } r_t;
