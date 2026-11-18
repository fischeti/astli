// `disable fork` and `wait fork` open no block, so a run that meets one ends
// at its `;` rather than taking the statements after it.
module m;
  task t;
    -> disable fork;
    a = 1;
    -> wait fork;
    b = 2;
  endtask
endmodule
