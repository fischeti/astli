// A macro is ALL_CAPS; UVM's `uvm_` macros are lower case, and so may a
// testbench's be that shares the prefix. A use of a macro is not its
// definition.
`define WIDTH 8
`define ASSERT_KNOWN(name, sig) assert property (!$isunknown(sig))
`define gfn get_full_name()
`define Width 8
`define uvm_my_info(msg) $display(msg)
`define UVM_MY_INFO(msg) $display(msg)
`define TC_SRAM_64x64 1
`undef gfn
module macros;
  initial $display(`gfn);
endmodule
