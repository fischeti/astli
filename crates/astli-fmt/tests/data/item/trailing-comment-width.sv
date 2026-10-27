// A comment at the end of a line does not count towards the width: code that
// fits without it stays on one line, and the comment runs past the width,
// since breaking the code would not shorten it. Code that does not fit even
// without its comment breaks as it would, with the comment after its last
// line. In a table, a comment that passes the width costs only its own
// column: its code still lines up, and the comments on either side line up
// apart, so that its code does not push their column right.
module m #(
  parameter int COUNT_B = 4
) ();
  localparam W_B = (COUNT_B > 1) ? $clog2(COUNT_B) : 1; // width of an index into the second dimension of the table
  assign a = b; // width of an index into the second dimension of the table, which is a longer comment
  assign result_of_the_long_computation = first_operand_of_it + second_operand_of_it + the_third_one; // sum
  string base_path = $sformatf("%s.%s", "tb.dut.u_aes_core.u_aes_control", // the control module
    "gen_fsm[0].gen_fsm_p.u_aes_control_fsm_i.u_aes_control_fsm"); // the FSM instance

  logic a; // short
  logic [WIDTH-1:0] a_much_longer_name_than_the_others_have_here = '0; // and a comment that cannot line up
  logic bb; // short too
  logic [3:0] dd; // and this one
  logic [7:0] ccc; // a comment long enough that lining it up would take its line past the width
endmodule
