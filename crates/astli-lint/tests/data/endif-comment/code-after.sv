// A comment added after an `endif with code after it on its line would hide
// that code, so there is no fix.
`ifdef FPGA
`endif module code_after; endmodule
