// A name is found in its own scope first, then in each around it: a block's
// variable hides the module's, a generate block sees the module's, a loop
// sees its own variable.
module m #(parameter int W = 4) (input logic [W-1:0] a, output logic y);
  typedef enum logic {OFF, ON} state_e;
  state_e state;
  logic x;
  always_comb begin : blk
    logic x;
    x = a[0];
    y = x | (state == ON);
    for (int i = 0; i < W; i++) y = y ^ a[i];
    if (missing) disable blk;
  end
  for (genvar g = 0; g < W; g++) begin : gen
    assign x = a[g];
  end
  function automatic logic f(input logic v);
    return v & x;
  endfunction
endmodule
