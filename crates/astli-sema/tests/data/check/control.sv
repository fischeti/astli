// A `return` gives a value only where its function returns one, and must
// there, but in a constructor; it never leaves a `fork`. A function runs in
// no time, so it cannot wait, but a `fork … join_none` it starts may.
module m;
  function void f(int a);
    return a;
  endfunction
  function int g(int a);
    fork
      $display("joins");
    join_any
    fork
      #10 $display("apart");
    join_none
    #1;
    @(a);
    wait (a);
    return a;
  endfunction
  function int h();
    return;
  endfunction
  task t;
    fork
      #20;
      return;
    join_none
  endtask
  task u;
    #1;
    return 1;
  endtask
  class c;
    function new();
      return;
    endfunction
  endclass
endmodule
