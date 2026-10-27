// A `;` after a blank goes with the blank, and one alone on its line with
// its line.
class spaced extends uvm_object;
  function void f();
    `uvm_info("test", "spaced", UVM_LOW) ;
    `uvm_info("test", "alone", UVM_LOW)
    ;
  endfunction
endclass
