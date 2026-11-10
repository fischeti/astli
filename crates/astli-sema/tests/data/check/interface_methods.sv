// Not reported: a module defines a method its interface port exports,
// `a.Read`, whose name is the interface's rather than a second `a`, and
// whose body uses the module's signals.
interface bus;
  extern task Read;
  modport target(export Read);
endinterface

module mem (bus.target a);
  logic avail;
  task a.Read;
    avail = 0;
  endtask
endmodule
