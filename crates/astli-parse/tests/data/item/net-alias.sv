// `alias` joins nets: two, or a chain of more, each a name, a select, a
// concatenation, or a net reached through an interface port.
module m (bus i);
  wire [31:0] a, b, c;
  wire [7:0] lo;
  alias a = b;
  alias {a[7:0], a[15:8], a[23:16], a[31:24]} = b = c;
  alias c[7:0] = lo;
  alias i.w = lo;
endmodule
