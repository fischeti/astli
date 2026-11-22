// A member of a struct a type parameter names goes to its declaration in
// the struct, written in place or by a macro, through an index, and a
// member of a member.
`define PAIR_T struct packed { logic x; logic y; }
//                                   ^

module type_parameters #(
  localparam type par_t = struct packed { logic x; logic y; },
//                                              ^
  parameter type mac_t = `PAIR_T,
  parameter type bus_t = struct packed { par_t a; logic valid; }
//                                             ^
) ();
  par_t       b;
  mac_t       c;
  par_t [1:0] f;
  bus_t       reqs [2];
  logic o1, o2, o3, o4;
  assign o1 = b.x;
//              ^
  assign o2 = c.x;
//              ^
  assign o3 = f[0].x;
//                 ^
  assign o4 = reqs[1].a.y;
//                    ^ ^
endmodule
