// Consecutive parameters line up in four columns: the keyword, the type, the
// name and the `=`, then a trailing comment. One without a type lines its name
// up with the others'. Rows that padding would take past the width split
// their table, and the rows on either side line up among themselves. A column
// lines up only among consecutive rows that need at most twelve spaces of
// padding for it, and a row that starts a new run in one column starts one
// in every column after it.
module modname #(
  parameter int Depth = 2048, // 8kB default
  localparam int Aw = $clog2(Depth), // derived parameter
  parameter type req_t = logic,
  parameter SimInit = "none",
  int unsigned NoKeyword = 1
) ();
endmodule

package p;
  localparam int unsigned INTERFACE_WIDTH = 64;  // Bits
  localparam int unsigned INTERFACE_WIDTH_BYTES = (INTERFACE_WIDTH + 7) / 8;
  localparam logic [3:0] Bar = 4'd4;
  parameter key_t Key = `KEY({
    A,
    B
  });
  logic x;
  localparam int After = 1;
endpackage
package q;
  localparam bit EnDecoupledRW = (WideRwDecouple != floo_pkg::None);
  localparam int unsigned NumVirtualChannels = (WideRwDecouple == floo_pkg::None) ? 1 : 2;
  localparam int unsigned NumWidePhysChannels = (WideRwDecouple == floo_pkg::Phys) ? 2 : 1;
  // Collective communication configuration
  localparam floo_pkg::collect_op_fe_cfg_t CollectOpCfg = RouteCfg.CollectiveCfg.OpCfg;
  localparam int Short = 1;
endpackage
package r;
  localparam logic [7:0] Mask = 8'hFF;
  localparam int Width = (FirstParameterName * SecondParameterNames) + ThirdParameterName_qq;
  localparam logic [7:0] Other = 8'h0F;
endpackage
