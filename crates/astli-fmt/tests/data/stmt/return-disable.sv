// `return` and `disable` take one space before what they name, and none
// before `;`.
module m;
  function automatic int f(int a);
    return   a+1 ;
  endfunction
  task t;
    disable   fork ;
    disable blk;
    return;
  endtask
endmodule
