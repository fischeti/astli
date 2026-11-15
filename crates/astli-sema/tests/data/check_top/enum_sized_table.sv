// A table of configurations sized by the enum that names them, so that a
// member without a row fails to build, holds its rows: an instance given a
// value read from one gives that value, which the warning shows.
module sub #(
  parameter int unsigned N
);
  $warning("N is %0d", N);
endmodule

module top;
  typedef enum int unsigned {A, B, C} config_e;
  parameter config_e Config = B;

  typedef struct packed {int unsigned n;} cfg_t;
  localparam cfg_t Cfgs[Config.num()] = '{'{1}, '{2}, '{3}};
  localparam cfg_t Cfg = Cfgs[Config];
  localparam int unsigned N = Cfg.n;
  localparam int unsigned Ms[Config.last() + 1] = '{4, 5, 6};

  sub #(.N(N)) i_sub ();
  sub #(.N(Ms[Config])) i_last ();
endmodule
