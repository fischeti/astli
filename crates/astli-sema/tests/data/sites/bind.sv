// A `bind` binds into the definition it names, or where none is so named,
// into an instance, which the name then uses. A bound instance's
// connections are names in what it binds into.
module checker_m (input logic a);
endmodule

module dut (input logic a);
endmodule

module top;
  logic b;
  dut u_dut (.a(b));
  bind dut checker_m u_check (.a);
//     ^   ^         ^         ^
  bind u_dut checker_m u_check_one (.a(a));
//     ^                               ^
endmodule
