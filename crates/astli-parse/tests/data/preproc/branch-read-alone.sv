// Each branch is read as if the file ended at its end: what follows the
// `endif cannot change how the branch is read.

// The last port of a branch is a name, though no `)` follows it, and the
// `endif after it is not.
module m (
    input a,
`ifdef WIDE
    b,
    c
`endif
);
endmodule

// A value of a branch among a case's values takes the comma after it, so
// the value after the region follows none.
module n;
  always_comb
    case (op)
      A,
`ifdef EXT
      B,
`endif
      C: y = 1;
      default: y = 0;
    endcase
endmodule

// A declaration of a ragged branch ends with the branch rather than taking
// the `endif and what comes after it.
package p;
`ifdef PACKED
  typedef struct packed {
    bit l;
`else
  typedef struct {
    rand bit l;
`endif
  } cfg_t;

  function automatic int f();
    return 0;
  endfunction
endpackage
