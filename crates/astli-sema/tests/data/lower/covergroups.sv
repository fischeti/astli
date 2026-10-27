// A covergroup declares its formals and `sample`'s arguments in a scope of
// its own, and its cover points and crosses in one within that: by label,
// or a cover point of a variable alone by the variable. Each keeps what it
// samples, its condition, its bins and its options. A bin keeps its count,
// its values, ranges, transitions or selection, and a `with` declares the
// iterator it runs over, `item` unless the call names it.
module m;
  logic clk, en;
  logic [3:0] a, b;
  int q[$];
  covergroup cg (int lo) @(posedge clk);
    option.per_instance = 1;
    type_option.weight = 2;
    cp_a: coverpoint a iff (en) {
      bins low[2] = {[lo:3]};
      bins even[] = {[4:$]} with (item % 2 == 0);
      bins seq = (1 => 2), (3 => [4:5]);
      ignore_bins other = default;
      illegal_bins big = cp_a with (item > 14);
      option.at_least = 4;
    }
    coverpoint b;
    bit [3:0] coverpoint a + b;
    ab: cross cp_a, b {
      bins x = binsof(cp_a.low) intersect {0} matches 2;
    }
  endgroup
  covergroup sampled with function sample (int v);
    coverpoint v;
  endgroup
  cg c = new(0);
  initial begin
    int s = q.sum() with (item * 2);
    int f[$] = q.find(x) with (x > s);
  end
endmodule
