// A function or task is laid out like a module, but its arguments follow
// its name with no space, as they do in a call, and stay on the line if they
// fit, unless a directive among them needs lines of its own. A prototype is
// its header alone.
package p;
function automatic logic [7:0] add (logic [7:0] a, logic [7:0] b);
    return a + b;
      endfunction : add
task t; input int a; @(posedge clk); endtask
class c;
extern virtual function int f(input int a, output b);
  pure virtual task run ( );
function new(string name = "c", uvm_component parent = null, int unsigned depth = 4, bit verbose = 0);
  super.new(name, parent);   // the base
endfunction
endclass
function int w(int a,
`ifdef WIDE
  int b = 2,
`endif
  int c = 3);
endfunction
function void c :: f();
endfunction
import "DPI-C" context function int dpi_f(input int a);
endpackage
