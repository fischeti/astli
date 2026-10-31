// A name used above its declaration is reported where it is used, with the
// declaration below; one declared above as well, around its scope, is not.
// A forward typedef declares its name from where it stands, a task or a
// block anywhere in its scope, and the head of `a.b` is not reported.
logic shadowed;
typedef class later_c;
module m;
  logic y;
  assign y = a & shadowed;
  logic a, shadowed;
  sub u (.i(n), .o());
  wire n;
  initial begin
    later_c handle;
    t();
    $display(blk.held);
    disable blk;
  end
  initial begin : blk
    logic held;
  end
  task t;
  endtask
endmodule

module sub(input logic i, output logic o);
endmodule

class later_c;
endclass

class c;
  function void set();
    count = 1;
  endfunction
  int count;
endclass
