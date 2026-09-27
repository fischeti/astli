// A covergroup is sampled on an event or by a `sample` function. Its cover
// points and crosses hold bins and options; bins count values, the values of
// a cover point `with` a condition, transitions, what no other bin counts, or
// in a cross a selection of other bins.
class c;
  covergroup cg with function sample (int a, bit [3:0] b);
    option.per_instance = 1;
    cp_a: coverpoint a iff (en) {
      bins low = {[0:3]};
      bins high[] = {[4:$]} with (item % 2 == 0);
      wildcard bins w = {4'b1??0};
      bins seq = (1 => 2 => [3:5] [*2]), (READ, PROG => READ);
      ignore_bins other = default;
      illegal_bins bad = cp_b with (item > 3);
    }
    cp_b: coverpoint b;
    ab: cross cp_a, cp_b {
      ignore_bins x = binsof(cp_a.low) intersect {0} && !binsof(cp_b);
      option.weight = 0;
    }
  endgroup : cg
endclass
module m;
  covergroup cg2 (int w) @(posedge clk);
    coverpoint x;
  endgroup
endmodule
