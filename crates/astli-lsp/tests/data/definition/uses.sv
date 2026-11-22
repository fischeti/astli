// A name used goes to its declaration, and a declaration to itself. An
// implicit net goes to its first use, and a name nothing declares goes
// nowhere.
package uses_pkg;
  localparam int Width = 4;
endpackage

module uses (input logic clk);
  logic [uses_pkg::Width-1:0] count;
//       ^         ^          ^
  always_ff @(posedge clk) count <= count + 1;
//                    ^             ^
  assign flag = 1'b1;
  assign sum = flag + missing;
//             ^      ^
  // A comment wider in bytes than in UTF-16: é𝄞
  assign /* é𝄞 */ sum = count;
//                 ^     ^
endmodule

// A name in what the parser kept as written goes nowhere, though it may
// be declared.
module opaque_uses;
  logic count;
  opaque #(count);
//         ^
endmodule
