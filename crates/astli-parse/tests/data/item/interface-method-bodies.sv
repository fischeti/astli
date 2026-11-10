// A module defines the task or function an interface port exports by naming
// the port before it, `task port.name`.
module mem (interface a);
  logic avail;
  task a.Read;
    avail = 0;
  endtask
  function void a.bar(int i, logic l);
  endfunction
endmodule
