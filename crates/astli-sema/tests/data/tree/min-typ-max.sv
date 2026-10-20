// An override given minimum, typical and maximum values takes the typical
// one, by name or by position.
module leaf #(parameter int I = 0) ();
endmodule

module top;
  leaf #(.I(7:8:9)) u_named ();
  leaf #(4:5:6) u_ordered ();
endmodule
