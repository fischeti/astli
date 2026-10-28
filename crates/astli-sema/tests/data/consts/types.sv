// A parameter's type: as written, or, with neither type nor range, its
// value's. Typedefs of integral types, enums and packed structs size as
// their bits; an enum member counts up from the one before it, and a name
// with a range declares a member per index.
package p;
  typedef logic [7:0] byte_t;
  typedef struct packed {
    logic [3:0] a;
    byte_t b;
  } pair_t;
  typedef union packed {
    logic [11:0] whole;
    pair_t parts;
  } either_t;
  typedef enum logic [1:0] {Idle, Busy = 2, Done} state_e;
  typedef enum {First, Second} plain_e;
  typedef enum logic [7:0] {Base = 8'h10, Many[3], Some[5:4], After} ranged_e;
  localparam byte_t A = 300;
  localparam B = $bits(pair_t);
  localparam C = $bits(either_t);
  localparam state_e D = Done;
  localparam E = $bits(state_e);
  localparam int unsigned F = 5;
  localparam signed [3:0] G = 4'hF;
  localparam [3:0] H = -1;
  localparam bit I = 2;
  localparam byte J = 8'hFF;
  localparam logic [3:0][7:0] K = 32'h1234_5678;
  localparam L = $bits(K);
  localparam shortint M = 'hFFFF;
  localparam real N = 3;
  localparam O = byte_t'(9'h1FF);
  localparam P = 4'(8'hAB);
  localparam Q = signed'(4'hF);
  localparam R = int'(2.5);
  localparam S = $clog2(1);
  localparam T = $clog2(5);
  localparam U = $clog2(1024);
  localparam V = $size(A);
  localparam W = $left(K);
  localparam X = $bits(logic [2:0]);
  localparam Y = $signed(4'hF);
  localparam Z = $countones(8'hF1);
  localparam AA = Some4;
  // A bound is an `int`, so one an unsigned zero minus one gives is -1.
  localparam int unsigned None = 0;
  localparam logic [None-1:0] AB = '1;
endpackage
