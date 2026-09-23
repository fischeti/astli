// A macro call is laid out as a call, but its arguments are text: each is
// written as it was read, only moved. A `;` after one stays on its line.
class c extends uvm_object;
  `uvm_object_utils   (c)
  task t;
    `uvm_info( `gfn , $sformatf("x %0d", `N(a)) , UVM_LOW )
    `DV_CHECK(a==b) ;
    `uvm_error(`gfn, $sformatf("unexpected response for address %0h with data %0h", address_q, data_q))
    `uvm_info(`gfn, $sformatf("first %0d",
                              first), UVM_HIGH)
    x = `M(,b) + `WIDTH;
  endtask
endclass
