// A test without a parameter list declares what it sweeps, and what `-G`
// sets, as body parameters, which are not reported. A test with a list, or
// a module that is no test, is.
(* astli_test *)
module tb_a;
  (* astli_sweep = "1, 2" *)
  parameter int unsigned N = 1;
  parameter int unsigned Cycles = 100;
  initial $finish;
endmodule

(* astli_test *)
module tb_b #(
  parameter int unsigned W = 1
);
  parameter int unsigned Local = 1;
endmodule

module helper;
  parameter int unsigned N = 1;
endmodule
