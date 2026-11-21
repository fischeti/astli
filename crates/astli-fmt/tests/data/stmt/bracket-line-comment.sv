// A line comment inside brackets breaks the line it ends, though nothing
// else breaks there. In a select, a call, a ternary or a concatenation,
// what follows it starts under the first entry, as it would outside
// brackets; a dimension's range has nothing to start under.
module m;
  assign a = b[c + // why
               d];
  assign e = f[g(h, // why
                 i)];
  logic [W - 1 : // why
         0] x;
  assign j = k[l ? // why
               m : n];
  assign o = p[{q, // why
                r}];
endmodule
