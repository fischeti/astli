// A call passes each argument as its port says; a system task reads its
// arguments but those it writes; a method may change what it is called on;
// a name in a region not lowered may be read or written.
module m;
  logic [7:0] x, y, z;
  int q [$];
  function automatic void f(input logic [7:0] a, output logic [7:0] b);
    b = a;
  endfunction
  initial begin
    f(x, y);
    $display("%0d", z);
    $sscanf("12", "%d", z);
    q.push_back(x);
    x++;
  end
  assert property (@(posedge x[0]) y |-> z);
endmodule
