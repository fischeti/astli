// A member a class and every class it extends lack, named `C::name`,
// `this.name`, `super.name` or through a handle; a name nothing around a
// class declares, nor the object an object's `randomize() with` names.
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

class registry #(type T = int);
endclass

class from_unknown extends uvm_object;
  function void f();
    this.anything = super.whatever + m_inherited;
  endfunction
endclass

module m;
  derived d, ds [2];
  from_unknown u;
  int limit;
  initial begin
    d.count = d.lost + ds[0].count + ds[1].vanished;
    void'(d.randomize() with { count < limit; absent > 0; });
    u.anything = u.whatever;
    void'(u.randomize() with { m_field < limit; });
    limit = registry #(int)::gone;
  end
endmodule
