// A class handle has its class's type, and so does a specialization's;
// `null` is a type of its own, and `?:` gives it the handle's. A member or
// a method reached through a handle, or through `this`, has the type its
// class declares; a parameterized class's parameters have no value, as
// each specialization gives its own, nor does what is computed from them.
package p;
  class item;
    int count;
    string name;
    function string label();
      return name;
    endfunction
  endclass

  class pool #(int Size = 4, type T = int);
    T entries [Size];
    logic [Size-1:0] used;
  endclass

  class user;
    item head;
    item other;
    pool #(8, item) items;
    int n;
    string s;

    function void run(bit c);
      head = null;
      other = c ? head : null;
      n = head.count;
      s = head.label();
      n = this.n + 1;
      s = this.s;
    endfunction
  endclass
endpackage
