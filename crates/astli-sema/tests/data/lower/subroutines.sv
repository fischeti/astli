// A function's arguments are its ports, and an argument without a
// direction is an input; a task may declare its arguments in its body.
module m;
  function automatic logic [3:0] f(int x, output y, z);
    int t;
    t = x;
    return t + 1;
  endfunction
  task automatic t;
    input a;
    output b;
    b = a;
  endtask
  function void g();
  endfunction
endmodule
