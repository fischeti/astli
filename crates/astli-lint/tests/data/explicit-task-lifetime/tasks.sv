// A task outside a class says `automatic` or `static`; a class's are
// automatic already.
module tasks;
  task automatic wait_a();
  endtask
  task wait_b();
  endtask
endmodule

class driver;
  task run();
  endtask
endclass
