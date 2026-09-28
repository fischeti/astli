// `new` before an object makes a shallow copy of it; `new` alone, with
// arguments or with a size is a constructor or an array.
class c;
  function void f();
    copy = new original;
    copy = new cfg.dut_cfg;
    copy = new templates[name];
    copy = new this;
    fresh = new;
    fresh = new(1, 2);
    arr = new[4];
  endfunction
endclass
