// A design without tops: a parameter's value is at the defaults of the
// definition around it. A package's has one body, so its value is the
// only one. A type parameter's value is a type.

package config_pkg;
  // Bytes in a word.
  parameter int unsigned WordBytes = 4;
//                       ^
endpackage

module fifo #(
  parameter int Depth = 8,
//              ^

  parameter type data_t = logic [7:0],
//               ^

  localparam int AddrWidth = $clog2(Depth)
//               ^
) ();
  typedef enum logic [1:0] {
    Empty,
    Full = 2'(Depth / 4)
  } fill_e;
  logic [AddrWidth-1:0] addr;
//       ^
  logic [config_pkg::WordBytes*8-1:0] word;
//       ^           ^
  fill_e fill;
  assign fill = Full;
//              ^
endmodule

// A value of an integer atom type is a plain number, with its sign; one
// with an unknown bit, or of any other type, is the literal it is.
package numbers_pkg;
  parameter int Negative = -1;
//              ^
  parameter byte Small = -2;
//               ^
  parameter longint unsigned Large = 64'd1 << 40;
//                           ^
  parameter integer Unset = 'x;
//                  ^
  parameter logic [7:0] Mask = 8'hF0;
//                      ^
endpackage
