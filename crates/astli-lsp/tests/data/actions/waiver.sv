// A waiver goes on the innermost node that takes one around the
// finding: a statement in `always_comb`, a declaration a macro names, or
// one that already waives another rule, whose string gains the rule. A
// `define is offered none, in a module's body or in a block, since the
// waiver would cover the whole body or block.
`define Name BadName

module waiver (
  input  logic a_i,
  output logic b_o
);
  `define lower_case 1
  (* astli_allow = "forbid-reg" *) reg q;
  logic `Name;
  always_comb begin
    `define lower_in_block 1
    b_o <= a_i;
  end
endmodule
