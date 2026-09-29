// A `;` after a `uvm_ macro call is reported; one after another macro, or a
// statement after the call, is not.
class test;
  task run();
    `uvm_info("test", "hello", UVM_LOW);
    `uvm_info("test", "hello", UVM_LOW)
    do_something();
    `MY_INFO("hello");
  endtask
endclass
