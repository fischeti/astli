// A select takes no space inside its brackets, even around operators, but
// `+:` and `-:` take one on either side. Nothing breaks inside it.
module m;
  assign a = data[ i ];
  assign b = data[WIDTH - 1 : 0];
  assign c = data[i * 8+:8];
  assign d = data[ top -: 4 ][0];
  assign e = mem[addr].field[idx + 1];
  assign f = q[$];
endmodule
