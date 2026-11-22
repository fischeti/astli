// A value is written by its type: a struct by member and an array by
// element, as an assignment pattern would, each in its own type's form; an
// enum by its member's name; a vector of 32 bits or more in hex.

package cfg_pkg;
  typedef enum logic [1:0] {
    Off,
    Slow,
    Fast
  } speed_e;

  typedef struct packed {
    int unsigned AddrWidth;
    int unsigned DataWidth;
    speed_e      Speed;
    logic [3:0]  Mask;
  } bus_cfg_t;

  typedef struct packed {
    bus_cfg_t   Bus;
    logic [7:0] Id;
  } port_cfg_t;

  parameter bus_cfg_t Bus = '{AddrWidth: 32, DataWidth: 4, Speed: Fast, Mask: 4'hA};
//                    ^

  parameter port_cfg_t Port = '{Bus: Bus, Id: 8'd7};
//                     ^

  parameter speed_e Speed = Slow;
//                  ^

  // No member has the value 3.
  parameter speed_e Odd = speed_e'(3);
//                  ^

endpackage

// A packed array by element, the first written the most significant, cut
// after four with a count of the rest.
package map_pkg;
  parameter logic [5:0][31:0] Base = {32'h0, 32'h1000_0000, 32'h2000_0000, 32'h3000_0000, 32'h4000_0000, 32'h5000_0000};
//                            ^

  parameter logic [1:0][31:0] Pair = {32'hdead_beef, 32'h0};
//                            ^

  // An unpacked array of structs, element by element.
  parameter cfg_pkg::bus_cfg_t Buses [2] = '{cfg_pkg::Bus, '{default: 0}};
//                             ^

endpackage

// A vector of 32 bits or more is hex, `_` every four digits. A digit whose
// bits are all `x` or all `z` is that letter; one that mixes known and
// unknown bits cannot be a hex digit, so the value is binary.
package wide_pkg;
  parameter logic [127:0] Key = 128'h0123_4567_89ab_cdef_0011_2233_4455_6677;
//                        ^

  parameter logic [35:0] Odd = 36'h9_8765_4321;
//                       ^

  parameter logic [31:0] Holes = 32'hffff_xxzz;
//                       ^

  parameter logic [31:0] Mixed = 32'b1111_0000_1111_0000_1111_0000_1111_00x0;
//                       ^

  parameter logic [63:0] Unset = 'x;
//                       ^

  parameter logic signed [39:0] Minus = -40'sd1;
//                              ^

  // Narrower than 32 bits stays the literal it is.
  parameter logic [15:0] Short = 16'hbeef;
//                       ^

endpackage

// An unpacked struct, with an enum member and a nested struct.
package unpacked_pkg;
  typedef struct {
    cfg_pkg::speed_e   Speed;
    int                Count;
    cfg_pkg::bus_cfg_t Bus;
  } top_cfg_t;

  parameter top_cfg_t Cfg = '{Speed: cfg_pkg::Off, Count: -3, Bus: cfg_pkg::Bus};
//                    ^

endpackage

// Cut: a struct after eight members, a value once it is long, and below
// three levels of aggregates.
package cut_pkg;
  typedef struct packed {
    logic [3:0] A, B, C, D, E, F, G, H, I, J;
  } ten_t;

  parameter ten_t Ten = '0;
//                ^


  typedef struct packed {
    logic [127:0] First, Second, Third, Fourth, Fifth;
  } long_t;

  parameter long_t Long = '1;
//                 ^


  typedef struct packed {logic [3:0] Leaf;} inner_t;
  typedef struct packed {inner_t Inner;} middle_t;
  typedef struct packed {middle_t Middle;} outer_t;
  typedef struct packed {outer_t Outer;} deep_t;

  parameter deep_t Deep = '0;
//                 ^

endpackage

// A struct with no name is cut to its first members in the type line; its
// declaration as written follows.
module anonymous (
  input struct packed {
    struct packed {
      logic [2:0] opcode;
      logic [2:0] param;
    } a;
    logic [7:0] b;
    logic [7:0] c;
    logic [7:0] d;
    logic [7:0] e;
  } port
//  ^

);
endmodule

// An enum's members are looked up in the body the value is, so a member
// whose value a parameter gives names it in each.
module levels #(parameter int Top = 3) ();
  typedef enum int {
    Low  = 0,
    High = Top
  } level_e;
  localparam level_e Max = High;
//                   ^

endmodule

// An instance's overrides are written as values are.
module sub #(
  parameter cfg_pkg::bus_cfg_t Cfg   = cfg_pkg::Bus,
  parameter cfg_pkg::speed_e   Speed = cfg_pkg::Off
) ();
endmodule

module top;
  sub #(
    .Cfg('{AddrWidth: 64, DataWidth: 8, Speed: cfg_pkg::Slow, Mask: '1}),
    .Speed(cfg_pkg::Fast)
  ) u_sub ();
//  ^

  levels #(.Top(5)) u_levels ();
endmodule
