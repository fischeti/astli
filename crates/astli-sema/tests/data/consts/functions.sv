// Constant functions run in a frame of their own: arguments, variables,
// loops, `case`, recursion, a return by the function's name. One that
// writes what is not its own, never ends, or recurses too deep is unknown.
package p;
  function automatic int unsigned idx_width(input int unsigned num_idx);
    return (num_idx > 32'd1) ? unsigned'($clog2(num_idx)) : 32'd1;
  endfunction

  function automatic int fact(int n);
    if (n <= 1) return 1;
    return n * fact(n - 1);
  endfunction

  function automatic int ones(logic [15:0] value);
    int count = 0;
    for (int i = 0; i < 16; i++) begin
      if (value[i]) count += 1;
    end
    return count;
  endfunction

  function automatic logic [3:0] pick(int sel);
    case (sel)
      0, 1: pick = 4'h1;
      2: pick = 4'h2;
      default: pick = 4'hF;
    endcase
  endfunction

  function automatic int sum(int n, int step = 1);
    int total;
    total = 0;
    while (n > 0) begin
      total += n;
      n -= step;
    end
    return total;
  endfunction

  function automatic logic [7:0] reversed(logic [7:0] value);
    for (int i = 0; i < 8; i++) reversed[i] = value[7 - i];
  endfunction

  function automatic int forever_loop(int n);
    while (1) n++;
    return n;
  endfunction

  function automatic int outside(int n);
    q = n;
    return n;
  endfunction

  localparam A = idx_width(1);
  localparam B = idx_width(17);
  localparam C = fact(5);
  localparam D = ones(16'hF0F1);
  localparam E = pick(1);
  localparam F = pick(7);
  localparam G = sum(10);
  localparam H = sum(10, 3);
  localparam I = reversed(8'b1100_0001);
  localparam J = forever_loop(0);
  localparam K = outside(1);
  localparam L = fact(12);
  localparam M = fact(200);
  int q;
endpackage
