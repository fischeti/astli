// A generate block may be named before its `begin`, in an `if`, a `for`, a
// `case` item, or on its own.
module m;
  if (P) gen_a : begin end else gen_b : begin end
  for (genvar i = 0; i < 2; i++) gen_c : begin end
  case (P)
    default: gen_d : begin end
  endcase
  gen_e : begin end
endmodule
