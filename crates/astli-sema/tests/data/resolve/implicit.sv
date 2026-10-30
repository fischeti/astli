// A name first connected to a port or assigned continuously declares a net,
// unless `default_nettype none forbids it; a use before the first counts
// too. So does each name a concatenation there is made of.
module m;
  assign z = w;
  sub u_sub (.a(w), .b(v));
  assign w = v;
  sub u_cat (.a({p, w[1:0], {2{q}}}));
  assign {r, s[0]} = 2'b0;
endmodule

`default_nettype none
module n;
  assign w = 1'b0;
  sub u_sub (.a(v));
endmodule
`default_nettype wire
