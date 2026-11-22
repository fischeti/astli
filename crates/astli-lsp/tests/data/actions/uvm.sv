// restriction: a `;` after a `uvm_ macro call is deleted.
class uvm_fix;
  task run();
    `uvm_info("test", "hello", UVM_LOW);
  endtask
endclass
