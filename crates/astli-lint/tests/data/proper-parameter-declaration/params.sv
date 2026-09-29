// A `parameter` belongs in a parameter list, of a module or a class; one in
// a body or a package is reported. A `localparam` belongs in a design
// element, a class or a package; one in the compilation unit is reported.
localparam int Unit = 1;

package pkg;
  parameter int InPackage = 1;
  localparam int Fine = 2;
endpackage

module params #(
  parameter int Width = 8,
  localparam int Half = Width / 2
) ();
  parameter int InBody = 1;
  localparam int AlsoFine = 2;
endmodule

class item #(parameter int Size = 4);
  localparam int Double = 2 * Size;
endclass
