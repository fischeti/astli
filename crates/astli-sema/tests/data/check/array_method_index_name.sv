// Not reported: SystemVerilog 2023 lets an array locator method name its
// index as well as its item, `find(item, index)`, and its `with` clause use
// both names.
function automatic void f;
  typedef struct {int index;} idx_type;
  idx_type arr1[4];
  idx_type arr2[$];
  arr2 = arr1.find(item, iter_index) with (item.index != iter_index);
endfunction
