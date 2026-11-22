// A member of a struct a type parameter names says what it is, as one of a
// typedef's struct does: through an index, and a member of a member.
module type_parameters #(
  localparam type par_t = struct packed { logic [3:0] x; logic y; },
  parameter type bus_t = struct packed { par_t a; logic valid; }
) ();
  par_t       b;
  par_t [1:0] f;
  bus_t       reqs [2];
  logic [3:0] o1, o3;
  logic       o4;
  assign o1 = b.x;
//              ^
  assign o3 = f[0].x;
//                 ^
  assign o4 = reqs[1].a.y;
//                    ^ ^
endmodule
