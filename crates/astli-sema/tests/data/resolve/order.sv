// A name is declared from where its declaration stands: above it, the name
// is looked for around the scope, so the file's `x` is found before the
// module's; and refers to the declaration below only if nothing else is
// found. A task, a function, a property and a block may be named above
// their declaration, a non-ANSI port's type may name what the body declares
// before it, and a method defined outside its class anything declared
// around it.
logic [7:0] x;
module m(p);
  logic y;
  assign y = x;
  logic x;
  assign y = z;
  logic z;
  initial begin
    t();
    y = f(z);
    disable blk;
  end
  assert property (prop);
  property prop;
    y;
  endproperty
  initial begin : blk
  end
  task t;
  endtask
  function logic f(logic v);
    return v;
  endfunction
  localparam int W = 2;
  input [W-1:0] p;
endmodule

class C;
  extern function int get();
  int k;
endclass
localparam int K = 1;
function int C::get();
  return k + K;
endfunction
