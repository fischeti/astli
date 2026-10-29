// A class's members are found in it, then in what it extends, then around
// it. `this` and `super` reach its members and its base's; `C::name` its
// members; a method or constraint defined outside it is in it. Past a base
// no file declares, a name declared around the class is found there, and
// one nothing declares is unknown.
package base_pkg;
  class base #(type T = int);
    T payload;
    int count;
    function new();
      count = 0;
    endfunction
  endclass
endpackage

package derived_pkg;
  import uvm_pkg::*;
  import base_pkg::*;
  parameter int Depth = 4;

  class packet extends base #(byte);
    typedef enum {Read, Write} kind_e;
    rand kind_e kind;
    rand int addr;
    static int made;
    constraint low_c { addr < Depth; }
    extern constraint aligned;
    extern function void bump(int by);
    function new();
      super.new();
      this.count = made;
      payload = 8'h0;
      kind = Read;
    endfunction
  endclass

  constraint packet::aligned { addr % 4 == 0; }

  function void packet::bump(int by);
    count += by;
    this.addr = super.count;
  endfunction

  class from_uvm extends uvm_object;
    function void report();
      int got = Depth + packet::made + m_inherited;
      packet::kind_e k = packet::Write;
      this.randomize();
    endfunction
  endclass
endpackage
