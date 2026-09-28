// A `randsequence` is a grammar walked at random: productions and the rules
// they may become, `|` between them, each weighted with `:=` and running
// its block when chosen. A rule's items are productions, calls with
// arguments, blocks, `if`, `repeat`, `case` and `rand join`.
function int f();
  int x;
  randsequence (main)
    main : first second | rand join (0.5) first second := 2 { x = 0; };
    first : if (x > 0) add(1) else add(2);
    second : repeat (3) add(x) | case (x) 0, 1 : third; default : first; endcase;
    third : { if (x == 1) break; x++; };
    void add(int y) : { x += y; };
    int value : { return 4; };
  endsequence
  return x;
endfunction
