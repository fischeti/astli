// An unpacked array takes an array of elements equivalent to its own, and a
// packed struct, a packed union or a packed array is equivalent to a vector
// of as many bits, as many states and the same signing; an enum only to
// itself. Rejected: a struct of other signing, of two states for four, of
// other width, and an enum.
typedef struct packed {
  logic a;
  logic b;
  logic c;
  logic d;
} nibble_t;
typedef struct packed signed { logic [1:0] hi; logic [1:0] lo; } snibble_t;
typedef struct packed { bit [3:0] bits; } bnibble_t;
typedef union packed { logic [3:0] whole; nibble_t parts; } either_t;
typedef enum logic [3:0] {Zero, One} code_e;

module pick #(parameter int N = 2) (
  input logic [3:0] in_i [N],
  output logic [3:0] out_o
);
  assign out_o = in_i[0];
endmodule

module top (output logic [3:0] o, output logic [3:0] p);
  nibble_t f [2];
  either_t g [2];
  nibble_t k [2];
  nibble_t [1:0] h [2];
  logic [7:0] bytes [2];
  logic [3:0] v [2];
  snibble_t s [2];
  bnibble_t b [2];
  nibble_t [2:0] wide [2];
  code_e e [2];

  assign f[0] = '0;
  assign f[1] = '1;
  pick #(.N(2)) u (.in_i(f), .out_o(o));

  always_comb begin
    v = g;
    bytes = h;
    k = v;
    v = s;
    v = b;
    bytes = wide;
    v = e;
  end

  pick #(.N(2)) u_signed (.in_i(s), .out_o(p));
endmodule
