// An enum takes only its own type: a compound assignment computes an
// integer, and a number given to an enum parameter, by default or override,
// needs a cast. A member's value is reported where the enum cannot take
// it: a sized literal of another width, an `x` or `z` in a 2-state enum,
// none after one with an `x` or `z`, one another member has. An untyped
// parameter whose value is text is text, as a string parameter takes.
typedef enum logic [1:0] {A, B, C} e_t;

module sub #(parameter e_t P = A, parameter string F = "") ();
endmodule

module m #(parameter Name = "rom", parameter e_t Q = 1);
  e_t v;
  enum logic [2:0] {G = 4'h2, H = 3'h3} wide;
  enum bit [1:0] {K = 0, L = 2'bxx, M = 1} two_state;
  enum integer {N = 0, O = {32{1'bx}}, R} after_x;
  enum logic [1:0] {S, T = 0} twice;
  sub #(.P(2), .F(Name)) u_number ();
  sub #(.P(C)) u_cast ();
  initial begin
    v += 1;
    v++;
    v = e_t'(v + 1);
  end
endmodule
