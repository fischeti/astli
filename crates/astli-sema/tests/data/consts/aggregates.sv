// Unpacked arrays and structs, packed structs and arrays by member and
// element, and type parameters. A pattern takes its shape from what it is
// assigned to: by position, by member name or index, by type, by default,
// which reaches into nested aggregates, and repeated.
package p;
  typedef struct packed {
    logic [3:0] hi;
    logic [3:0] lo;
  } pair_t;
  typedef struct {
    int unsigned width;
    bit enable;
    pair_t pair;
  } cfg_t;
  parameter type word_t = logic [15:0];

  localparam int A [3] = '{1, 2, 3};
  localparam int B [4] = '{2{5, 6}};
  localparam int C [0:3] = '{1: 7, default: 0};
  localparam int D [2:0] = '{4, 5, 6};
  localparam E = D[0];
  localparam int F [2] = {8, 9};
  localparam pair_t G = '{hi: 4'hA, lo: 4'h5};
  localparam H = G.hi;
  localparam pair_t I = '{default: 4'h3};
  localparam cfg_t J = '{width: 32, enable: 1'b1, pair: '{4'h1, 4'h2}};
  localparam K = J.width + 1;
  localparam L = J.pair.lo;
  localparam cfg_t M = '{default: 0};
  localparam cfg_t N = '{bit: 1'b0, default: 1};
  localparam O = $bits(cfg_t);
  localparam P = $bits(A);
  localparam Q = $size(A);
  localparam logic [1:0][3:0] R = '{4'hC, 4'hD};
  localparam S = R[1];
  localparam T = $bits(word_t);
  localparam word_t U = '1;
  localparam V = $left(R, 2);
  localparam W = A[5];
  localparam pair_t X = pair_t'(8'h96);
  localparam Y = X.lo;
  localparam string Z [2] = '{"ab", "c"};

  function automatic int sum(int values [3]);
    int total = 0;
    for (int i = 0; i < 3; i++) total += values[i];
    return total;
  endfunction

  function automatic pair_t swap(pair_t value);
    swap.hi = value.lo;
    swap.lo = value.hi;
  endfunction

  localparam AA = sum(A);
  localparam AB = swap(G);
endpackage
