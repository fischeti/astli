// A name declared twice in one scope is reported at the second. A forward
// typedef, an `extern` constraint or method with its definition, the label
// two arms of a generate `if` share, and one name in two scopes are not.
module m;
  logic v;
  wire v;
  typedef int t;
  typedef logic t;
  sub u ();
  sub u ();
  typedef class c;
  class c;
    extern function void f();
    extern constraint k;
  endclass
  function void c::f(); endfunction
  constraint c::k { 1; }
  if (1) begin : g
    logic x;
  end else begin : g
    logic x;
  end
endmodule

module sub;
  logic v;
endmodule
