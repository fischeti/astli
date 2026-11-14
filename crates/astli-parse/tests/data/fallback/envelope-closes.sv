// An encrypted envelope may hold what closes the constructs around it, as a
// vendor encrypts a module through its `endmodule`: what is still open at
// the end of the file is then closed, and the module keeps its header.
module enc (input logic a, output logic y);
  always begin
`pragma protect begin_protected
`pragma protect data_block
VGhlIGJvZHksIGVuZCBhbmQgZW5kbW9kdWxlIGdvIGhlcmUu
`pragma protect end_protected
