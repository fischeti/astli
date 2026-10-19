// The built-in `std` package, which no file declares: its members, written
// `std::name` or imported, are known, and a name it lacks is an error. Its
// wildcard import supplies only its own names, so `y` is still undeclared.
module m;
  int x;
  std::semaphore sem;
  initial begin
    void'(std::randomize(x));
    void'(std::process::self());
    void'(std::nothing_here(x));
  end
endmodule

module n;
  import std::*;
  import std::mailbox;
  import std::bogus;
  int x;
  assign x = y;
endmodule
