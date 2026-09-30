// Expressions keep their structure: every operation is parenthesised in
// the dump.
module m;
  assign a = b + c * d;
  assign e = f ? g[3:0] : h[i +: 2];
  assign {j, k} = {2{l}} | ~m;
  assign n = pkg::F(o, .p(1)) + $bits(q) + int'(r) + 8'(s) + signed'(t);
  assign u = '{default: '0, x: 1};
  assign v = w inside {1, [2:3]};
  assign x = {<<8 {y}};
  assign z = s_t'{a: 1, b: 2};
  assign aa = bb.cc[0].dd;
  assign ee = $clog2(W) - 1;
  assign ff = \escaped + gg;
endmodule
