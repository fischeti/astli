// The nets an `alias` joins resolve as any name does, an undeclared one
// reported, and the alias keeps nothing else in its module from being
// checked.
module m;
  wire [7:0] a, b;
  alias a = b = c;
  alias {a[3:0], a[7:4]} = b;
  assign a = d;
endmodule
