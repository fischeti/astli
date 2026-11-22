// A struct's member, used or declared, an import's package and item, and
// a `bind`'s target say what they refer to. The comment opening a file is
// above the first declaration, unless a blank line is between them.
package bus_pkg;
  typedef struct packed {
    // Where to.
    logic [31:0] addr;
  //             ^

    logic        write; // Or read.
  } req_t;
endpackage

module checker_m (input logic clk);
endmodule

module bus;
  import bus_pkg::req_t;
//       ^        ^
  req_t req;
  logic [31:0] where;
  assign where = req.addr;
//                   ^
  assign req.write = 1'b0;
//           ^
endmodule

bind bus checker_m u_check (.clk(1'b0));
//   ^
