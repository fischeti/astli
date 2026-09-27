// A comment `astli-fmt: skip` before an item has it written as it was read,
// moved as a block to where it stands. It ends the table it would be in.
module m;
  logic a;
  // astli-fmt: skip
  logic   [7:0]    hand_aligned;
  logic [3:0] b;

      /* astli-fmt: skip */
      assign x = { a,  b,
                   c,  d };
  assign yy=a;
  assign z=b;

  always_comb begin
    // astli-fmt: skip
    if (a)   q = 1;  else   q = 0;
    r=1;
  end
endmodule

// astli-fmt: skip
module   kept ( input a,
                input b );
endmodule

// Only the comment that says exactly that.
// astli-fmt: skip, please
module   formatted;
endmodule
