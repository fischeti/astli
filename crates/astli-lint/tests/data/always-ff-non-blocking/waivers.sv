// A waiver covers the construct it stands on and nothing outside it: a
// statement, a block, a module. It may name the rule or its group, several
// at once. A name that is neither, or a value that is not a string of names,
// is reported, and waives nothing.
module waivers (
  input  logic clk_i,
  input  logic d_i,
  output logic q_o
);
  always_ff @(posedge clk_i) begin
    (* astli_allow = "always-ff-non-blocking" *) q_o = d_i;
    q_o = d_i;
  end

  (* astli_allow = "correctness" *)
  always_ff @(posedge clk_i) begin
    q_o = d_i;
    q_o = d_i;
  end

  (* astli_allow = "always-comb-blocking, always-ff-non-blocking" *)
  always_ff @(posedge clk_i) q_o = d_i;

  (* astli_allow = "always-comb-blocking" *)
  always_ff @(posedge clk_i) q_o = d_i;

  (* astli_allow = "always-ff-non-blocknig" *)
  always_ff @(posedge clk_i) q_o = d_i;

  (* astli_allow *) always_ff @(posedge clk_i) q_o = d_i;
  (* astli_allow = "" *) always_ff @(posedge clk_i) q_o = d_i;
  (* keep, astli_allow = "suspicious lowrisc" *) always_ff @(posedge clk_i) q_o = d_i;
endmodule

(* astli_allow = "always-ff-non-blocking" *)
module waived_whole (
  input  logic clk_i,
  input  logic d_i,
  output logic q_o
);
  always_ff @(posedge clk_i) q_o = d_i;
endmodule
