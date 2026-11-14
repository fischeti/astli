// What an encrypted envelope holds is unknown, and may close the module and
// the include guard around it, as vendors encrypt a file's body to its end:
// each module keeps the ports its header declares, and nothing is reported.
module top (input logic a, output logic y, output logic z);
  vcs u_vcs (.a(a), .y(y));
  enc u_enc (.a(a), .y(z));
endmodule

module vcs (input logic a, output logic y);
`protected
Qd0]S8;2F_LO0?8bgN5W4b=Q<#\6.T2LF^a/&M<[K7d0Gc=3[R1-YH7>B?F7$Q
`endprotected
endmodule

`ifndef ENC_SV
`define ENC_SV
module enc (input logic a, output logic y);
`pragma protect begin_protected
`pragma protect data_block
VGhlIGJvZHksIGVuZG1vZHVsZSBhbmQgdGhlIGNsb3NpbmcgZW5kaWYgZ28gaGVyZS4=
`pragma protect end_protected
