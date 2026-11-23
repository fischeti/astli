// A type parameter given a named type with packed dimensions, `nib_t
// [15:0]`, is that type's width times theirs, not the width of a
// part-select of the name; a part-select of a value stays one.
package p;
  typedef logic [3:0] nib_t;
endpackage

module pass #(
  parameter type data_t = logic
) (
  input  data_t data_i,
  output data_t data_o
);
  assign data_o = data_i;
endmodule

module top #(
  parameter int Ratio = 1,
  parameter logic [7:0] Mask = 8'hff
) (
  input  logic [15:0][3:0] a_i,
  output logic [15:0][3:0] a_o,
  input  logic [3:0] n_i,
  output logic [3:0] n_o,
  input  logic [1:0][15:0][3:0] w_i,
  output logic [1:0][15:0][3:0] w_o,
  input  logic [3:0] m_i,
  output logic [3:0] m_o
);
  typedef logic [3:0] nib_t;

  // 4 × 16 bits, and 4 × 1, which fit.
  pass #(.data_t(nib_t [15:0])) u_a (.data_i(a_i), .data_o(a_o));
  pass #(.data_t(nib_t [Ratio-1:0])) u_n (.data_i(n_i), .data_o(n_o));
  // 4 × 16 × 2 bits, which fit.
  pass #(.data_t(p::nib_t [1:0][15:0])) u_w (.data_i(w_i), .data_o(w_o));
  // 128 bits into 64.
  pass #(.data_t(nib_t [15:0])) u_trunc (.data_i(w_i), .data_o());
  // 4 bits, the part-select's.
  pass #(.data_t(logic [$bits(Mask[3:0])-1:0])) u_m (.data_i(m_i), .data_o(m_o));
endmodule
