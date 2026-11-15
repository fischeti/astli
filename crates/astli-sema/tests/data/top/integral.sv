// A single bit takes 0 and 1, signed or not; a wider vector takes no list,
// but any number its width and signedness hold, up to 64 bits. Past that,
// and for a type that is no integral one, no value has a short spelling.
module tb;
  parameter bit Enable = 0;
  parameter logic Valid = 1;
  parameter logic [0:0] One = 0;
  parameter bit signed Sign = 0;
  parameter logic [7:0] Byte = 0;
  parameter byte Small = 0;
  parameter int unsigned Depth = 4;
  parameter longint Long = 0;
  parameter logic [63:0] Wide = 0;
  parameter logic [64:0] Wider = 0;
  parameter real Ratio = 1.0;
  parameter string Name = "";
  typedef struct packed { logic a; logic b; } pair_t;
  parameter pair_t Pair = '0;
  parameter int Table [2] = '{1, 2};
  parameter Implicit = 3;
endmodule

//> values tb Enable
//> values tb Valid
//> values tb One
//> values tb Sign
//> values tb Byte
//> set tb Byte=255 Small=-128 Depth=4294967295
//> set tb Byte=256
//> set tb Byte=-1
//> set tb Small=127
//> set tb Small=128
//> set tb Small=-129
//> set tb Long=-9223372036854775808
//> set tb Long=9223372036854775808
//> set tb Wide=18446744073709551615
//> set tb Wide=18446744073709551616
//> set tb Depth=1_000
//> set tb Depth=0x10
//> set tb Enable=One
//> set tb Implicit=-7
//> values tb Wider
//> values tb Ratio
//> values tb Name
//> values tb Pair
//> values tb Table
