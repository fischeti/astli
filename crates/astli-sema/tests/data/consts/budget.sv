// A constant function's budget of a million statements covers the functions
// it calls, but not a parameter it reads, whose value is the same whoever
// asks for it first. `Early` asks `Late` midway through a run of 700,000
// statements, and `Late` takes 400,000 of its own.
package early;
  function automatic int asks();
    int spent;
    spent = late::spin(350000);
    return spent + late::Late;
  endfunction
  localparam int Early = asks();
endpackage

package late;
  // Two statements a pass: the pass, and its empty statement.
  function automatic int spin(int n);
    repeat (n) ;
    return n;
  endfunction
  localparam int Late = spin(200000);
endpackage
