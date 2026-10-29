// A forward typedef announces a type its scope declares later, which a
// name finds instead, so a class named before its body is that class, its
// members found through it. One its scope never declares is still found.
package p;
  typedef class later;
  typedef class never;

  class user;
    int got = later::count;
    later handle;
    never other;
    function void f();
      handle.count = 1;
    endfunction
  endclass

  class later;
    static int count;
  endclass
endpackage
