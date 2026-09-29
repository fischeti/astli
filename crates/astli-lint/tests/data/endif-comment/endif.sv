// An `endif closing an `ifdef or `ifndef names its macro in a comment on its
// line, `//` or `/* */`. A comment naming another macro, or on the next
// line, does not count; an `if closes with no single macro to name.
`ifdef SIM
`endif // SIM

`ifndef SYNTHESIS
`else
`endif /* SYNTHESIS */

`ifdef FPGA
`endif

`ifdef ASIC
`endif // FPGA

`ifdef VERILATOR
`endif
// VERILATOR
