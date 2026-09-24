// The brackets last in a `foreach` header name the loop's variables, any of
// which may be left out; everything before them is the array, selects and all.
initial begin
  foreach (cfg.words[j, k]) x = 1;
  foreach (banks[, bank]) x = 1;
  foreach (seeds[bit'(part)][i]) x = 1;
end
