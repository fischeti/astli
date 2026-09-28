// A number's base takes its digits. A sign goes before the whole number, so
// `8'd-6` is a base with none; the rest here are numbers, one in pieces and
// one with its digits from a macro.
module m;
  initial begin
    a = 8'd-6;
    b = -8'd6;
    c = 8 'h FF;
    d = 8'h`DIGITS;
  end
endmodule
