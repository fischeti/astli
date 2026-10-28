// `inside` is 1 if its operand matches a value of the set or falls in a
// range, `x` if it matches none but may match one. An `x` or `z` in a
// value matches any bit, `$` leaves a bound open, and an unpacked array's
// elements are values of the set. All are compared at the widest of them.
package p;
  localparam int Table[3] = '{3, 4, 5};
  localparam bit A = 3 inside {1, [2:4]};
  localparam bit B = 5 inside {1, [2:4]};
  localparam bit C = 3 inside {[5:1]};
  localparam logic D = 4'b1010 inside {4'b1?10};
  localparam logic E = 3'bz11 inside {3'b1?1, 3'b011};
  localparam bit F = 12 inside {[10:$]};
  localparam bit G = 12 inside {[$:3]};
  localparam bit H = 4 inside {1, Table};
  localparam bit I = 8'hFF inside {-1};
  localparam bit J = 10 inside {[8 +/- 2]};

  // `case … inside` takes its labels as such a set.
  function automatic int f(int x);
    case (x) inside
      [1:3]: return 10;
      [$:0], 7: return 20;
      default: return 30;
    endcase
  endfunction
  localparam int K = f(2);
  localparam int L = f(-4);
  localparam int M = f(5);
endpackage
