// `begin_keywords reserves only the keywords of the version it names, until
// its `end_keywords; pairs nest. `logic` and the 1364-2005 `uwire` are names
// under 1364-2001, `config` under its noconfig variant, and `logic` is a
// keyword again once the pair ends. A version the standard does not define
// keeps the one in force.
`begin_keywords "1364-2001"
module a;
  reg logic;
`begin_keywords "1364-2001-noconfig"
  wire config;
`end_keywords
  wire uwire;
endmodule
`end_keywords
`begin_keywords "1999-9999"
module b;
  logic x;
endmodule
`end_keywords
