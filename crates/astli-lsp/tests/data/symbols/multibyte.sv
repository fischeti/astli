// A character is counted in UTF-16 code units: `é` is one and `😀` two.
// A name is ASCII, or escaped.
module m /* é */ (input logic a /* ä */, output logic \😀q );
  /* 😀 */ logic b;
  wire \café , c;
  sub u_sub (.x("é"), .y(b)); /* 😀 */ sub u_two ();
endmodule
