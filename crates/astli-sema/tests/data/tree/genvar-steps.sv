// A generate loop's header steps its genvar as a constant function's
// statement would, the value an integer: an increment, a compound
// assignment sized as the assignment it is, an assignment of the genvar.
// `i /= Two` is unsigned, as `Two` is, so -8 steps past 0 at once. A value
// with an `x` or `z`, and a step of anything but the genvar, leave the loop
// undecided.
module top #(parameter int Step = 3) ();
  localparam logic [3:0] Two = 4'd2;
  genvar g, h;
  for (genvar i = 2; i >= 0; i--) begin : g_down end
  for (genvar i = 1; i < 20; i <<= 2) begin : g_shift end
  for (genvar i = 1; i < 30; i *= Step) begin : g_times end
  for (genvar i = -8; i < 0; i /= Two) begin : g_unsigned end
  for (g = 0; g < 4; g = g + 2) begin : g_assigned end
  for (genvar i = 'x; i < 4; i++) begin : g_unknown_first end
  for (genvar i = 0; i < 4; i = 'x) begin : g_unknown_step end
  for (h = h + 1; h < 2; h++) begin : g_reads_itself end
  for (genvar i = 0; i < 3; g++) begin : g_steps_another end
endmodule
