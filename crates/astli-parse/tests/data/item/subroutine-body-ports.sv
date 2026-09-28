// A function or a task may declare its ports in its body, the way Verilog
// did, in a module or a class alike.
module m;
  function integer f;
    input a;
    input [7:0] b;
    f = a + b;
  endfunction
  task t;
    input x;
    output logic y;
    y = x;
  endtask
endmodule
class c;
  function integer g;
    input a;
    g = a + 42;
  endfunction
endclass
