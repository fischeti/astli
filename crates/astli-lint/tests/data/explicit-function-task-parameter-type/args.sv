// Each argument has a type, in the header or the body. One with neither a
// direction nor a type takes the one before it's; a range alone is no type.
package args;
  function automatic int f(input logic a, input b, c);
    return 0;
  endfunction
  function automatic int g(input int a, b);
    return 0;
  endfunction
  function automatic int h(input [7:0] a);
    return 0;
  endfunction
  task automatic t;
    input d;
    output logic e;
  endtask
endpackage
