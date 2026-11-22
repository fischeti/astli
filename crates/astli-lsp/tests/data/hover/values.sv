// Where the design has tops, a parameter's value is said at the defaults
// and for each distinct value the bodies the tops build take, with the
// path of an instance that has it, a handful of them and a count of the
// rest. Where every body agrees with the defaults it is said once.

module same #(parameter int W = 1) ();
//                          ^
endmodule

module two #(parameter int W = 1) ();
//                         ^

  localparam int Double = 2 * W;
//               ^
endmodule

module many #(parameter int W = 1) ();
//                          ^
endmodule

module unbuilt #(parameter int W = 1) ();
//                             ^
endmodule

module top;
  same u_same_a ();
  same #(.W(1)) u_same_b ();
  two u_two_a ();
  two #(.W(3)) u_two_b ();
  two #(.W(3)) u_two_c ();
  many #(.W(2)) u_many_2 ();
  many #(.W(3)) u_many_3 ();
  many #(.W(4)) u_many_4 ();
  many #(.W(5)) u_many_5 ();
  many #(.W(6)) u_many_6 ();
  many #(.W(7)) u_many_7 ();
endmodule
