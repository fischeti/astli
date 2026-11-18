// Only a `matches` after a cross bin's selection counts the bins it needs;
// inside the brackets of a `with`, and anywhere else in a covergroup, it
// matches a pattern.
covergroup cg;
  cp: coverpoint a {
    bins b[] = {[0:15]} with (item matches 3);
  }
  ab: cross cp, b {
    bins s = binsof(cp) with (cp matches 1) matches 2;
  }
endgroup
