// An override, or a parameter's value, that is not a literal is hinted
// with what it evaluates to, as hover writes values; a literal, a type,
// and a value written as it is said need none. A long value is cut. A
// parameter of `unit`, which has parameters and no top builds, has none:
// only its defaults are known.

package cfg;
  typedef enum logic [1:0] { Slow, Fast } speed_e;
  typedef struct packed {
    logic [31:0] base;
    logic [31:0] mask;
    speed_e      speed;
  } region_t;
  localparam int Lanes = 4;
  localparam int Bits = Lanes * 8;
  localparam logic [31:0] Base = 32'h1000 << 16;
  localparam region_t Region = '{base: Base, mask: 32'hffff, speed: Fast};
  localparam speed_e Default = Fast;
  localparam int Plain = 8;
  localparam int Widths[3] = '{8, 16, Lanes};
  localparam int Table[3] = '{8, 16, 32};
  localparam logic [15:0] Joined = {8'd1, 8'd2};
endpackage

module unit #(
  parameter int Width = 8,
  parameter int Depth = Width * 2,
  parameter type T = logic [Width-1:0],
  parameter cfg::region_t Region = '0
) ();
  localparam int Half = Depth / 2;
endmodule

module wrap;
  unit #(.Width(cfg::Bits), .Region(cfg::Region)) u_named ();
  unit #(cfg::Lanes + 1) u_positional ();
  unit #(.Width(16), .T(logic)) u_literal ();
endmodule
