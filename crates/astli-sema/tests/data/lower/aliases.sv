// An `alias` joins two nets or a chain of them, each a name, a select, a
// concatenation, or a net reached through an interface port.
module m (bus i);
  wire [7:0] a, b, c;
  alias a = b = c;
  alias {a[3:0], a[7:4]} = b;
  alias i.w = c;
endmodule
