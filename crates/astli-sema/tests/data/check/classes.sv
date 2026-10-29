// A member a class and every class it extends lack, named `C::name`,
// `this.name` or `super.name`; a name nothing around a class declares.
// Past a base no file declares, any name may be inherited, so none is
// reported.
class base;
  int count;
endclass

class derived extends base;
  function void f();
    count = this.count + super.count + base::count;
    this.missing = super.absent + derived::gone + nowhere;
    this.randomize();
  endfunction
endclass

class from_unknown extends uvm_object;
  function void f();
    this.anything = super.whatever + m_inherited;
  endfunction
endclass
