// A call may carry attributes between its name and its arguments.
module m;
  function void func();
  endfunction
  initial func (* note *) ();
endmodule
