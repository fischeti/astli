// A constraint is lower_snake_case ending in `_c`, with no empty word, in its
// declaration and its `extern` prototype alike.
class item;
  rand int size;
  constraint size_c { size < 8; }
  constraint c_size { size > 0; }
  constraint Size_c { size != 3; }
  constraint size__c { size != 4; }
  extern constraint mask_w_PutFullData_c;
endclass
