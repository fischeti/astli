// An assignment pattern may be keyed by an integer or real type, setting
// every member of that type, beside `default` and member names.
module m;
  initial begin
    s = '{default: 1, int: 0};
    s = '{int: 0, logic: 1'b1, real: 1.0};
    s = '{a: 1, my_t: 2};
  end
endmodule
