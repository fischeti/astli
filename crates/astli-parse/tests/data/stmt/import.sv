// A block, a function or a task imports into its own scope.
function automatic int f();
  import p::*;
  return 0;
endfunction

initial begin
  import p::x, q::y;
  $display(x);
end
