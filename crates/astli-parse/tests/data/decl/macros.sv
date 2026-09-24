// A macro may stand for the declared name.
logic [31:0] `X(mcause);
// A macro that writes several enum names writes the commas between them too,
// so the list must not insist on one after it.
typedef enum { A, `MORE(x) B } e;
// A macro may supply a literal's digits.
parameter int unsigned Base = 32'h`DM_ADDR;
