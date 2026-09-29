// `$random`, `$dist_*`, `$psprintf` and `$srandom` are reported, called or
// not; `$urandom` and `$sformatf` are what to use instead.
module random;
  int a;
  string s;
  initial begin
    a = $random;
    a = $random(seed);
    a = $dist_uniform(seed, 0, 9);
    s = $psprintf("%0d", a);
    $srandom(3);
    a = $urandom;
    s = $sformatf("%0d", a);
  end
endmodule
