// With more than one parameter, each is set by name; one may go by position.
module params;
  sub #(8, 4) u_a ();
  sub #(.Width(8), 4) u_b ();
  sub #(.Width(8), .Depth(4)) u_c ();
  sub #(8) u_d ();
endmodule
