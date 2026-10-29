// A handle's member is one its class declares or inherits, through a
// typedef, an element of an array of handles, or a chain of handles; an
// array's are its methods, which sema does not model. A
// method through a handle is the class's, and an object's `randomize`
// finds its members before what is around the call, but after a
// `foreach`'s variable; `local::name` is around the call. Past a class no
// file declares, a member is unknown.
package pkt_pkg;
  class header;
    rand bit [3:0] len;
  endclass

  class base;
    rand int addr;
  endclass

  class packet extends base;
    rand header hdr;
    rand int data [4];
    function void send(input int at, output bit ok);
      ok = at == addr;
    endfunction
  endclass

  typedef packet packet_t;
endpackage

module m;
  import uvm_pkg::*;
  import pkt_pkg::*;
  int limit, i;
  bit ok;
  packet p;
  packet_t ps [2];
  packet q [$];
  uvm_object u;
  initial begin
    p = new;
    p.addr = p.hdr.len + ps[1].addr;
    p.send(limit, ok);
    q.push_back(p);
    limit = q.size() + q[0].addr;
    void'(p.randomize() with { addr < limit; hdr.len > 0; foreach (data[i]) data[i] < i; });
    void'(u.randomize() with { anything < local::limit; });
    u.m_field = p.missing;
  end
endmodule
