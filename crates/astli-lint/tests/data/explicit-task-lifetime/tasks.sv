// A task outside a class says `automatic` or `static`; a class's are
// automatic already, and a DPI import or export takes no lifetime.
module tasks;
  task automatic wait_a();
  endtask
  task wait_b();
  endtask
  import "DPI-C" context task wait_c();
  export "DPI-C" task wait_a;
endmodule

class driver;
  task run();
  endtask
endclass
