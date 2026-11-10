// SystemVerilog 2023: `default` among a constructor's arguments stands for
// the base constructor's, passed on by `super.new(default)` or by the
// `extends` clause.
class A;
  function new(int x);
  endfunction
endclass

class B extends A;
  function new(int size, default);
    super.new(default);
  endfunction
endclass

class C extends A (default);
  function new(default, bit enable);
  endfunction
endclass
