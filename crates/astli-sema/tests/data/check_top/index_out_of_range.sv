// A constant index into an unpacked array of constants has an element
// there, from either bound, and so does each element of a constant slice;
// one outside the range is an error at the index, where it is evaluated: a
// branch a constant condition never takes is not. A packed vector's bit and
// a variable array are read outside their range as the standard says, with
// no finding, and so is an index that is not constant.
module top (input int i);
  typedef enum int unsigned {A, B, C, D} config_e;
  parameter config_e Config = D;

  localparam int unsigned Ns[3] = '{1, 2, 3};
  localparam int unsigned Rs[3:1] = '{1, 2, 3};
  localparam int unsigned Last = Ns[2];
  localparam int unsigned Past = Ns[Config];
  localparam int unsigned Negative = Ns[-1];
  localparam int unsigned Below = Rs[0];
  localparam int unsigned Top = Rs[3];
  localparam int unsigned Guarded = Config < 3 ? Ns[Config] : 0;
  localparam bit Shorted = Config < 3 && Ns[Config] > 0;
  localparam bit Either = Config >= 3 || Ns[Config] > 0;

  localparam logic [3:0] V = 4'b1010;
  localparam logic Bit = V[7];
  localparam int unsigned Inside[2] = Ns[1:2];
  localparam int unsigned Beyond[2] = Ns[2:3];
  localparam int unsigned Up[2] = Ns[1+:2];
  localparam int unsigned UpBeyond[2] = Ns[2+:2];
  localparam int unsigned Down[3] = Ns[2-:3];
  localparam int unsigned DownBeyond[2] = Rs[1-:2];

  int unsigned vars[3];
  int unsigned x, y, z;
  always_comb begin
    x = vars[3];
    y = Ns[i];
    if (Config < 3) z = Ns[Config];
    else z = Ns[Config - 1];
  end
endmodule
