// What typing rejects in what is built: an enum given anything but its own
// type, an unpacked struct or array given another shape, a string given a
// number, an operator given a real, a string or a handle it does not take;
// in an assignment, an initialiser, a connection either way, an argument
// and a `return`, a type parameter's type among them. What keeps the enum,
// a cast, a pattern and an array's concatenation are fine.
package p;
  typedef enum logic [1:0] {Idle, Busy, Done} state_e;
  typedef struct { logic valid; logic [7:0] data; } entry_t;

  function automatic state_e next(state_e s);
    return s == Done ? Idle : 2'd1;
  endfunction
endpackage

module sub import p::*; (
  input state_e state_i,
  output logic [7:0] row_o [2]
);
  assign row_o = '{default: '0};
endmodule

module top import p::*; #(parameter type T = logic [1:0]) (input logic c);
  state_e s, t, u = 2'd0;
  logic [1:0] v;
  logic [7:0] w;
  logic [7:0] row [2];
  logic [7:0] copy [2];
  logic [7:0] wide [3];
  entry_t e;
  real r;
  string str;
  int n;
  T x;

  always_comb begin
    s = c ? Idle : Busy;
    t = s | t;
    s = state_e'(v);
    s = v;
    s = c ? s : 2'd2;
    e = '{valid: 1'b1, data: w};
    e = w;
    copy = {w, w};
    copy = wide;
    n = r & 1;
    n = str * 2;
    str = w;
    str = "idle";
    t = next(v);
    s = x;
  end

  sub u_ok (.state_i(s), .row_o(row));
  sub u_bad (.state_i(v), .row_o(w));
endmodule
