// A branch among a randsequence's productions holds productions, and a
// branch in a production's block holds statements.
function void f();
  randsequence (main)
    main : first;
`ifdef X
    first : { x = 1; };
`else
    first : {
`ifdef Y
      x = 2;
`endif
    };
`endif
  endsequence
endfunction
