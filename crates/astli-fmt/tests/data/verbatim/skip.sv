// An item carrying `(* astli_fmt_skip *)` is written as it was read, moved as
// a block to where it stands. It ends the table it would be in.
module m;
  logic a;
  (* astli_fmt_skip *)
  logic   [7:0]    hand_aligned;
  logic [3:0] b;

      (* astli_fmt_skip *) assign x = { a,  b,
                                        c,  d };
  assign yy=a;
  assign z=b;

  always_comb begin
    (* astli_fmt_skip *)
    if (a)   q = 1;  else   q = 0;
    r=1;
  end
endmodule

(* astli_fmt_skip *)
module   kept ( input a,
                input b );
endmodule

// Among other attributes too.
(* keep, astli_fmt_skip *)
module   also_kept;
endmodule

// A comment asks for nothing, nor does another attribute.
// astli-fmt: skip
(* astli_fmt_skipped *)
module   formatted;
endmodule
