// An enum parameter takes its members, in the order declared, whether its
// type is a package's or the module's own, and is set by a member's name or
// by the number that is a member's value. A name or a number no member has
// is refused, each in its own way.
package fifo_pkg;
  typedef enum logic [1:0] { Registered, Bypass, Fall = 2'd3 } mode_e;
endpackage

module tb_fifo;
  parameter fifo_pkg::mode_e Mode = fifo_pkg::Registered;
  typedef enum int { Low = -4, High = 100 } level_e;
  parameter level_e Level = Low;
endmodule

//> values tb_fifo Mode
//> values tb_fifo Level
//> set tb_fifo Mode=Bypass
//> set tb_fifo Mode=3 Level=High
//> set tb_fifo Level=-4
//> set tb_fifo Mode=Through
//> set tb_fifo Mode=2
//> set tb_fifo Mode=fifo_pkg::Bypass
