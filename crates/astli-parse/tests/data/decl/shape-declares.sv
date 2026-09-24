// Two names in a row are a declaration because nothing else is written that
// way, not because the first was resolved. The typedef makes no difference.
typedef int my_t;
my_t x;
unknown_t x;
uvm_reg_data_t mask [4];
// `const` can only begin a declaration.
const uvm_reg_data_t mask = 0;
int x;
